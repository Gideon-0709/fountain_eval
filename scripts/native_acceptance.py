#!/usr/bin/env python3
"""Exercise the installed CLI with framework-owned smoke and failure fixtures."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=Path("target/debug/feval"))
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--faults", action="store_true", help="Check deliberately failing fixtures")
    parser.add_argument("--reproduction", action="store_true", help="Check frozen execution and cache integrity")
    args = parser.parse_args()
    work = args.work.resolve()
    if work.exists():
        raise SystemExit("Use a new work directory; previous evidence is not overwritten")
    work.mkdir(parents=True)
    cli = work / "feval"
    shutil.copy2(args.cli.resolve(), cli)
    env = {**os.environ, "XDG_STATE_HOME": str(work / "state")}
    subprocess.run([str(cli), "init"], cwd=work, env=env, check=True, stdout=subprocess.DEVNULL)
    workspace = work / "feval_workspace"
    runs = workspace / "results"
    cache = workspace / ".fountain-eval/cache"
    reports = workspace / "reports"
    starter_path = workspace / "fountain-eval.toml"
    starter = starter_path.read_text()
    ledger = []

    def command(label, arguments, expected, interrupt=False):
        if arguments[0] in ["run", "prepare"]:
            arguments = [*arguments, "--json"]
        started = time.monotonic()
        before = set(runs.glob("*"))
        with (work / (label + ".stdout")).open("w") as out, (work / (label + ".stderr")).open("w") as err:
            process = subprocess.Popen([str(cli), *map(str, arguments)], cwd=work, env=env, stdout=out, stderr=err)
            if interrupt:
                deadline = time.monotonic() + 180
                while process.poll() is None and time.monotonic() < deadline:
                    if any(list(p.glob("C*/*/logs/validate.stdout")) for p in set(runs.glob("*")) - before):
                        process.send_signal(signal.SIGINT)
                        break
                    time.sleep(.05)
            code = process.wait(timeout=7200)
        text = (work / (label + ".stdout")).read_text()
        body = json.loads(text) if text.strip() and arguments[0] not in ["report", "compare"] else {}
        if "result" in body:
            raw = Path(body["result"])
            assert raw.is_dir() and not (reports / raw.name).exists()
            published = json.loads(subprocess.check_output([str(cli), "report", raw.name, "--json"], cwd=work, env=env, text=True))
            report = Path(published["report"])
            saved = json.loads(subprocess.check_output([str(cli), "report", str(report), "--json"], cwd=work, env=env, text=True))
            entries = saved["batch"]["entries"]
            assert len(entries) == 1, "The acceptance fixtures use one selection each"
            assert len(saved["components"]) == 1
            markdown = Path(published["markdown"])
            assert markdown == report / "report.md" and markdown.stat().st_size > 0
            assert set(report.iterdir()) == {markdown, report / "index.json.lzma"}
            assert not (runs / saved["batch"]["run_id"]).exists(), "Verified reports must clean temporary raw evidence"
            assert not (cache / saved["batch"]["run_id"]).exists(), "Batch cache must be removed"
            body = {"report":str(report), "batch":saved["batch"], "result_ref":str(report)+"#C0", "result":saved["components"][0]}
            subprocess.run([str(cli),"report",str(report)],cwd=work,env=env,stdout=subprocess.DEVNULL,check=True)
        elif "prepared" in body:
            directory = Path(body["prepared"])
            entry = body["batch"]["entries"][0]
            record = json.loads((directory / entry["directory"] / "result.json").read_text())
            body.update(result=record, result_ref=str(directory), prepared=str(directory))
            assert not (reports / body["batch"]["run_id"]).exists()
        row = dict(label=label, exit=code, seconds=round(time.monotonic()-started, 3))
        if "result" in body:
            progress = (work / (label + ".stderr")).read_text()
            record = body["result"]
            assert "C0 [1/1]" in progress and "READY   " in progress, label
            state = "PASSED" if record["status"] == "succeeded" else record["status"].replace("_", " ").upper()
            assert f"C0 {state}" in progress, label
            assert "[VALIDATE" not in progress and "[MEASURE" not in progress, label
            assert not any(line.startswith("  LIVE") and "/rq/" in line for line in progress.splitlines()), label
            if record["status"] == "succeeded":
                assert "  ERROR " not in progress and "  FATAL " not in progress, label
                assert f"DONE  Validation: {record['validation_count']}/{record['validation_count']} checks" in progress, label
                assert f"DONE  Performance: {record['sample_count']}/{record['sample_count']} samples" in progress, label
                assert f"DONE  Diagnostics: {record['diagnostic_count']}/{record['diagnostic_count']} observations" in progress, label
                assert all(f"[{i}/4] {name}" in progress for i, name in enumerate(["Preparation", "Validation", "Performance", "Diagnostics"], start=1)), label
            if any(event.get("severity") == "fatal" and event.get("status") == "failed" for event in record["correctness"]["fatal_failures"]):
                assert "  FATAL " in progress, label
            if record["status"] in ["failed", "interrupted"]:
                assert record["error"] and f"Reason: {record['error'].split('; see ', 1)[0].splitlines()[0][:80]}" in progress, label
            row.update(run=body["result_ref"], status=body["result"]["status"], stage=body["result"]["stage"], error=body["result"].get("error"))
        ledger.append(row)
        (work / "ledger.json").write_text(json.dumps(ledger, indent=2)+"\n")
        print(json.dumps(row), flush=True)
        if code != expected:
            print((work / (label + ".stderr")).read_text()[-12000:], flush=True)
        assert code == expected, row
        return body

    baseline = command("starter", ["run"], 0)
    record = baseline["result"]
    assert record["status"] == "succeeded"
    assert record["planned_measurements"] > 0
    assert record["sample_count"] == record["planned_measurements"] * record["plan"]["repetitions"]
    assert all(row["validation"]["status"] != "not_executed" for row in record["coverage"]["cases"])
    command("starter-report", ["report", baseline["report"]], 0)
    markdown = (work / "starter-report.stdout").read_text()
    assert "## Summary" in markdown and "### Key results · Findings" in markdown
    assert "Fatal · None detected in completed checks." in markdown
    command("starter-comparison", ["compare", baseline["result_ref"], baseline["result_ref"]], 0)

    if args.reproduction:
        pending = command("prepare-frozen", ["prepare"], 0)
        try:
            starter_path.write_text("invalid replacement made after prepare\n")
            repeated = command("run-frozen", ["run", "--prepared", pending["prepared"]], 0)
            assert repeated["result"]["quadruple"] is not None
            command("compare-frozen", ["compare", baseline["result_ref"], repeated["result_ref"]], 0)
            command("invalid-replacement", ["run"], 2)
        finally:
            starter_path.write_text(starter)
        pending = command("prepare-cache-check", ["prepare"], 0)
        with Path(pending["result"]["built"]["binary"]).open("ab") as binary:
            binary.write(b"corruption fixture")
        rejected = command("corrupt-cache", ["run", "--prepared", pending["prepared"]], 4)
        assert "checksum" in rejected["result"]["error"]

    if args.faults:
        fault_checks(command, workspace, work, tomllib.loads(starter))

    print(f"Native framework acceptance finished: {work}", flush=True)


def fault_checks(command, workspace, work, starter):
    # Build the toy operator against the trait dependency chosen by the CLI starter.
    engine_ref = starter["engine0"]
    package, version = engine_ref.removeprefix("registry:").rsplit("@", 1)

    def fixture(name, injection):
        root = work / "generated-fixtures" / name
        (root / "src").mkdir(parents=True)
        (root / "Cargo.toml").write_text(f"""[package]
name = "acceptance-operator"
version = "0.2.0"
edition = "2024"

[dependencies.fountain_engine]
package = {json.dumps(package)}
version = {json.dumps("=" + version)}

[features]
default = ["default-marker"]
default-marker = []
fixture-type = []
""")
        (root / "src/lib.rs").write_text(f"""
#[cfg(all(feature = "fixture-type", not(feature = "default-marker")))]
pub mod ops {{
    use fountain_engine::{{traits::DataOperator, types::Operation}};
    pub struct FixtureOperator {{ bytes: Vec<u8> }}
    impl FixtureOperator {{
        pub fn new(width: usize) -> Self {{
            {injection}
            Self {{ bytes: vec![0; width] }}
        }}
    }}
    impl DataOperator for FixtureOperator {{
        fn get_vector(&self, _: usize) -> &[u8] {{ &self.bytes }}
        fn execute(&mut self, _: &Operation) {{}}
    }}
}}
""")
        subprocess.run(["git", "init", "--quiet", str(root)], check=True)
        return f"path:{root}#ops::FixtureOperator"

    def run(label, operator=None, *, engine=engine_ref, test=starter["test"],
            scheme=starter["scheme"], defaults=False, expected=4, interrupt=False):
        config = workspace / f"{label}.toml"
        text = f"engine0 = {json.dumps(engine)}\nscheme = {json.dumps(scheme)}\ntest = {json.dumps(test)}\n"
        if operator:
            enabled = "true" if defaults else "false"
            text += (
                f"operator0 = {{ source = {json.dumps(operator)}, features = [\"fixture-type\"], "
                f"default-features = {enabled} }}\n"
            )
        config.write_text(text)
        if interrupt:
            prepared = command(f"{label}-prepare", ["prepare", "--config", config], 0)
            return command(label, ["run", "--prepared", prepared["prepared"]], expected, interrupt=True)["result"]
        return command(label, ["run", "--config", config], expected)["result"]

    gated = run("disabled-operator-type", fixture("gated-operator", ""), defaults=True)
    assert gated["stage"] == "build" and "ops" in "\n".join(gated["failure_details"])
    wrong = run("wrong-output", fixture("wrong-output", ""))
    assert wrong["stage"] == "validate" and wrong["sample_count"] == 0
    assert wrong["correctness"]["fatal_failures"]
    checks = [row for row in wrong["coverage"]["cases"] if row["validation"]["status"] != "skipped"]
    for row in checks:
        is_validation = any(family.startswith("V") for family in row["family"].split(","))
        expected = {"passed", "failed"} if is_validation else {"not_executed"}
        assert row["validation"]["status"] in expected
    graph = wrong["build"]["dependency_graph"]["driver"]
    fixture_id = next(package["id"] for package in graph["packages"] if package["name"] == "acceptance-operator")
    features = next(node["features"] for node in graph["resolve"]["nodes"] if node["id"] == fixture_id)
    assert features == ["fixture-type"]

    crash = run("crash", fixture("crash", "std::process::exit(23);"))
    assert crash["stage"] == "validate" and crash["sample_count"] == 0
    protocol = run("protocol", fixture("protocol", 'println!("invalid event"); std::process::exit(0);'))
    assert protocol["stage"] == "validate" and "driver protocol" in protocol["error"]
    unknown = work / "generated-fixtures/unknown-engine"
    (unknown / "src").mkdir(parents=True)
    (unknown / "Cargo.toml").write_text(f'[package]\nname = {json.dumps(package)}\nversion = "9.0.0"\nedition = "2024"\n')
    (unknown / "src/lib.rs").write_text("")
    subprocess.run(["git", "init", "--quiet", str(unknown)], check=True)
    run("unknown-interface", engine=f"path:{unknown}", expected=3)
    hung = fixture("hang", "std::thread::sleep(std::time::Duration::from_secs(900));")
    interrupted = run("interrupt", hung, expected=130, interrupt=True)
    assert interrupted["status"] == "interrupted"
    run("missing-reference", engine=f"path:{work / 'missing-engine'}")
    run("unknown-test", test="9.0.0", expected=3)
    run("unknown-scheme", scheme="missing", expected=3)


if __name__ == "__main__":
    main()
