#!/usr/bin/env python3
"""Run the whole formal catalog and audit component failures without hiding coverage.

Every invocation retains a unique result. No retries or upstream edits. Known
component defects count as verified rejection, never as positive test coverage.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

ENGINE = "registry:fountain_engine@2.1.0"
OPERATORS = "git+https://github.com/shhyang/fountain_operators.git?package=fountain_operators&rev=ce232222603876f3c033109c548cf1091e874c10"


def operator_reference(source, operator):
    rust_type = {"slab": "SlabDataOperator", "simd": "SimdDataOperator"}[operator]
    return {"source": f"{source}#{rust_type}", "features": ["simd"]}


def reference_toml(reference):
    if isinstance(reference, str):
        return json.dumps(reference)
    fields = ", ".join(f"{name} = {json.dumps(value)}" for name, value in reference.items())
    return "{ " + fields + " }"


def active_features(graph, package):
    package_id = next(p["id"] for p in graph["packages"] if p["name"] == package)
    return next(node["features"] for node in graph["resolve"]["nodes"] if node["id"] == package_id)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=Path("target/debug/feval"))
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--monorepo", type=Path)
    parser.add_argument("--arena", type=Path)
    parser.add_argument("--faults", action="store_true")
    parser.add_argument("--reproduction", action="store_true", help="Also execute a second whole run from immutable preparation")
    parser.add_argument("--expect-upstream-x86-failure", action="store_true")
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
    runs, cache, reports = workspace / "results", workspace / ".fountain-eval/cache", workspace / "reports"
    ledger, results, attempts = [], {}, []

    def command(label, arguments, expected=None, interrupt=False):
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
            assert len(entries) == 1, "This helper inspects one C; multi-C contracts are in tests/batch.rs"
            assert len(saved["components"]) == 1
            assert report.stat().st_size > 0 and report.name == "report.md"
            assert list(report.parent.iterdir()) == [report]
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
        if expected is not None:
            if code != expected:
                print((work / (label + ".stderr")).read_text()[-12000:], flush=True)
            assert code == expected, row
        return body, code

    def run(label, engine, operator, scheme="rq", action="run", expected=None, interrupt=False, test="0.1.0"):
        config = workspace / (label + ".toml")
        config.write_text((f"engine0={reference_toml(engine)}\n" if engine else "") + (f"operator0={reference_toml(operator)}\n" if operator else "") + f"scheme={json.dumps(scheme)}\ntest={json.dumps(test)}\n")
        body, code = command(label, [action, "--config", config], expected, interrupt)
        if "result" in body: attempts.append(body)
        if body.get("result", {}).get("status") == "succeeded":
            record = body["result"]
            assert code == 0 and not record["correctness"]["fatal_failures"]
            assert record["sample_count"] == record["planned_measurements"] * 15
            for row in record["summary"]:
                if row["case"].startswith("workflow/decode/") and "/m0/" in row["case"]:
                    k = int(row["case"].split("/")[3][1:])
                    assert row["incomplete"] == 0
                    assert row["workload"]["received_repairs"]["max"] == 0
                    assert row["workload"]["received_sources"]["min"] == k
            assert all(c["validation"]["status"] != "not_executed" for c in record["coverage"]["cases"])
            command(label+"-report", ["report", body["report"]], 0)
            markdown = (work / (label + "-report.stdout")).read_text()
            assert "## Validation and warnings" in markdown and "## Coverage and measurement context" in markdown
            results[label] = body
        elif body.get("result", {}).get("status") != "prepared":
            assert not body.get("result", {}).get("summary"), "Failure published a complete performance summary"
        return body, code

    def known_operator(label, engine, operator, scheme):
        body, code = run(label, engine, operator, scheme)
        build = body["result"]["build"]
        assert build["components"]["implementation"] == operator["source"].split("#", 1)[1]
        assert "simd" in active_features(build["dependency_graph"]["driver"], "fountain_operators")
        if code == 0:
            return
        record = body["result"]
        failures = record["correctness"]["fatal_failures"]
        assert code == 4 and record["stage"] == "validate", ledger[-1]
        assert any("MulAdd" in v.get("message", "") and "scalar: 0" in v.get("message", "") for v in failures), failures
        ledger[-1]["classification"] = "Verified fatal GF mul-add-zero defect; later cases remain not executed; no positive whole-suite coverage"
        (work / "ledger.json").write_text(json.dumps(ledger, indent=2)+"\n")

    def known_arena(label, engine, arena, scheme):
        body, code = run(label, engine, arena, scheme)
        if code == 4:
            record = body["result"]
            failures = record["correctness"]["fatal_failures"]
            assert record["stage"] == "validate" and record["sample_count"] == 0, ledger[-1]
            assert any(
                v.get("case", "").startswith("correctness/receive-contract/")
                and "V10 missing-with-duplicates" in v.get("message", "")
                and "expected Some(" in v.get("message", "")
                and "actual Some(" in v.get("message", "")
                for v in failures
            ), failures
            ledger[-1]["classification"] = "V10 repeated-input corruption; no measured coverage"
            (work / "ledger.json").write_text(json.dumps(ledger, indent=2)+"\n")
        else:
            assert code == 0, ledger[-1]
        return body, code

    for scheme in ["rq", "r10"]:
        engine = {"source": ENGINE, "default-features": scheme == "r10"}
        body, _ = run("public-engine-only-"+scheme, engine, None, scheme, expected=0)
        graph = body["result"]["build"]["dependency_graph"]["driver"]
        driver_package = next(p for p in graph["packages"] if p["name"] == "fountain-eval-driver")
        engine_edge = next(d for d in driver_package["dependencies"] if d["name"] == "fountain_engine")
        assert engine_edge["uses_default_features"] == engine["default-features"]

    if args.expect_upstream_x86_failure:
        import platform
        assert platform.machine() in ["x86_64", "AMD64"]
        for op in ["slab", "simd"]:
            body, _ = run("x86-"+op, ENGINE, operator_reference(OPERATORS, op), expected=4)
            assert body["result"]["stage"] == "build"
            log = "\n".join(body["result"]["failure_details"])
            assert "E0433" in log and "portable" in log
        print("Verified pinned upstream x86 operator build rejection; engine-only planning covered separately", flush=True)
        return

    arena = f"path:{args.arena.resolve()}#MemoryDataOperator" if args.arena else None
    fallback = arena or operator_reference(OPERATORS, "slab")
    if args.reproduction and not arena:raise SystemExit("--reproduction requires an authorized --arena checkout")
    arena_results = {}
    for scheme in ["rq", "r10"]:
        for op in ["slab", "simd"]:
            known_operator("public-"+op+"-"+scheme, ENGINE, operator_reference(OPERATORS, op), scheme)
        if arena:arena_results[scheme] = known_arena("public-arena-"+scheme, ENGINE, arena, scheme)
    if args.monorepo:
        mono = args.monorepo.resolve()
        for scheme in ["rq", "r10"]:
            for op in ["slab", "simd"]:
                known_operator("dev-"+op+"-"+scheme, f"path:{mono/'fountain_engine'}", operator_reference(f"path:{mono/'fountain_operators'}", op), scheme)
            for op in ["slab", "simd", *(["arena"] if arena else [])]:
                run("product-"+op+"-"+scheme, f"path:{mono/'fountain_bin_ffi'}", arena if op=="arena" else operator_reference(f"path:{mono/'fountain_operators'}", op), scheme, expected=0)
        if arena:
            body, _ = run("dev-arena-conflict", f"path:{mono/'fountain_engine'}", arena, expected=4)
            assert "conflict" in body["result"]["error"]
    for scheme in ["rq", "r10"]:
        matching = [v for key, v in results.items() if key.endswith("-"+scheme)]
        for index, other in enumerate(matching):
            common = {s["case"] for s in matching[0]["result"]["summary"]} & {s["case"] for s in other["result"]["summary"]}
            command(f"compare-{scheme}-{index}", ["compare", matching[0]["result_ref"], other["result_ref"]], 0 if common else 2)
            if not common:
                assert "no common measured cases" in (work/f"compare-{scheme}-{index}.stderr").read_text()
    command("different-scheme", ["compare", results["public-engine-only-rq"]["result_ref"], results["public-engine-only-r10"]["result_ref"]], 2)

    if args.reproduction:
        # The frozen run is independent of later changes to a local component.
        first, first_code = arena_results["rq"]
        checkout=work/"generated-fixtures/arena"
        shutil.copytree(args.arena.resolve(),checkout,ignore=shutil.ignore_patterns(".git","target",".fountain-eval"))
        subprocess.run(["git","-C",str(checkout),"init","--quiet"],check=True)
        snapshot_operator = f"path:{checkout}#MemoryDataOperator"
        prepared,_=run("snapshot",ENGINE,snapshot_operator,action="prepare",expected=0)
        lib=checkout/"src/lib.rs"
        lib.write_text('compile_error!("changed original after preparation");\n'+lib.read_text())
        repeated,_=command("prepared-whole-run",["run","--prepared",prepared["prepared"]],first_code)
        if first_code == 0:
            command("prepared-compare",["compare",first["result_ref"],repeated["result_ref"]],0)
        else:
            first_failure = first["result"]["correctness"]["fatal_failures"][0]
            repeated_failure = repeated["result"]["correctness"]["fatal_failures"][0]
            assert [first_failure[k] for k in ("case", "message")] == [repeated_failure[k] for k in ("case", "message")]
            command("prepared-failed-comparison",["compare",first["result_ref"],repeated["result_ref"]],2)
        failed,_=run("changed-input",ENGINE,snapshot_operator,expected=4)
        assert failed["result"]["stage"]=="build"
        # A consumed preparation is removed; create an independent frozen input for corruption.
        lib.write_text(lib.read_text().removeprefix('compile_error!("changed original after preparation");\n'))
        prepared,_=run("corrupt-snapshot",ENGINE,snapshot_operator,action="prepare",expected=0)
        binary=Path(prepared["result"]["built"]["binary"])
        with binary.open("ab") as out:out.write(b"corruption fixture")
        corrupt,_=command("corrupt-cache",["run","--prepared",prepared["prepared"]],4)
        assert "checksum" in corrupt["result"]["error"]

    if args.faults:
        def fixture(name, injection):
            root=work/"generated-fixtures"/name
            (root/"src").mkdir(parents=True)
            (root/"Cargo.toml").write_text('[package]\nname="acceptance-operator"\nversion="0.2.0"\nedition="2024"\n[dependencies]\nfountain_engine="=2.1.0"\n[features]\ndefault=["default-marker"]\ndefault-marker=[]\nfixture-type=[]\n')
            (root/"src/lib.rs").write_text('#[cfg(all(feature="fixture-type", not(feature="default-marker")))] pub mod ops {use fountain_engine::{traits::DataOperator,types::Operation};pub struct FixtureOperator{v:Vec<u8>}impl FixtureOperator{pub fn new(n:usize)->Self{'+injection+'Self{v:vec![0;n]}}}impl DataOperator for FixtureOperator{fn get_vector(&self,_:usize)->&[u8]{&self.v}fn execute(&mut self,_:&Operation){}}}')
            subprocess.run(["git","-C",str(root),"init","--quiet"],check=True)
            return {"source": f"path:{root}#ops::FixtureOperator", "features": ["fixture-type"]}
        gated = fixture("gated-operator", "")
        body,_=run("disabled-operator-type",ENGINE,{**gated,"default-features":True},expected=4)
        assert body["result"]["stage"]=="build"
        assert "ops" in "\n".join(body["result"]["failure_details"])
        for name, injection in [("wrong-output", ""), ("crash", "std::process::exit(23);"), ("protocol", 'println!("invalid event");std::process::exit(0);')]:
            body,_=run(name,ENGINE,fixture(name,injection),expected=4)
            assert body["result"]["stage"]=="validate"
            graph = body["result"]["build"]["dependency_graph"]["driver"]
            assert active_features(graph, "acceptance-operator") == ["fixture-type"]
            if name=="protocol":assert "driver protocol" in body["result"]["error"]
        unknown=work/"generated-fixtures/unknown-engine"
        (unknown/"src").mkdir(parents=True)
        (unknown/"Cargo.toml").write_text('[package]\nname="fountain_engine"\nversion="9.0.0"\nedition="2024"\n')
        (unknown/"src/lib.rs").write_text("")
        subprocess.run(["git","-C",str(unknown),"init","--quiet"],check=True)
        run("unknown-interface",f"path:{unknown}",fallback,expected=3)
        hung=fixture("hang", "std::thread::sleep(std::time::Duration::from_secs(900));")
        body,_=run("interrupt",ENGINE,hung,expected=130,interrupt=True)
        assert body["result"]["status"]=="interrupted"
        # A short timeout and process-group cleanup are separately exercised by tests/processes.rs.
        run("missing-reference","path:/fountain-eval-nonexistent",fallback,expected=4)
        run("unknown-test",ENGINE,fallback,test="9.0.0",expected=3)
        run("unknown-scheme",ENGINE,fallback,scheme="missing",expected=3)

    planned=[body["result"] for body in attempts if body["result"].get("coverage",{}).get("cases")]
    registered={c["case"] for c in planned[0]["coverage"]["cases"]}
    applicable={c["case"] for r in planned for c in r["coverage"]["cases"] if c["validation"]["status"] != "skipped"}
    reached={c["case"] for r in planned for c in r["coverage"]["cases"] if c["validation"]["status"] in ["passed","failed"]}
    positive={c["case"] for r in planned for c in r["coverage"]["cases"] if c["validation"]["status"] == "passed" or (c["validation"]["status"] == "failed" and c["validation"].get("severity") == "warning")}
    (work/"reachability.json").write_text(json.dumps({"registered":len(registered),"reached":len(reached),"passed_or_warning":len(positive),"applicable_not_executed":sorted(applicable-reached),"applicable_without_positive_evidence":sorted(applicable-positive),"inapplicable_for_all_selected_integrations":sorted(registered-applicable),"note":"Fatal-blocked cases are coverage gaps, not passes; outside-domain rows remain explicit applicability exclusions"},indent=2)+"\n")
    print(f"Framework acceptance finished; inspect ledger and reachability for component failures and coverage gaps: {work}",flush=True)


if __name__=="__main__":
    main()
