#!/usr/bin/env python3
"""Run the whole formal catalog and audit component failures without hiding coverage.

Every invocation retains a unique result. No retries or upstream edits. Known
component defects count as verified rejection, never as positive test coverage.
"""
import argparse
import json
from pathlib import Path
import shutil
import signal
import subprocess
import time

ENGINE = "registry:fountain_engine@2.1.0"
OPERATORS = "git+https://github.com/shhyang/fountain_operators.git?package=fountain_operators&rev=ce232222603876f3c033109c548cf1091e874c10"


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
    runs, cache = work / "runs", work / "cache"
    ledger, results, attempts = [], {}, []

    def command(label, arguments, expected=None, interrupt=False):
        if arguments[0] in ["run", "prepare"]:
            arguments = [*arguments, "--json"]
        started = time.monotonic()
        before = set(runs.glob("*"))
        with (work / (label + ".stdout")).open("w") as out, (work / (label + ".stderr")).open("w") as err:
            process = subprocess.Popen([str(cli), *map(str, arguments)], cwd=work, stdout=out, stderr=err)
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
        if "batch" in body:
            batch_dir = Path(body["run_dir"])
            entries = body["batch"]["entries"]
            assert len(entries) == 1, "This helper inspects one C; multi-C contracts are in tests/batch.rs"
            entry = entries[0]
            assert entry["directory"], entry
            child = batch_dir / entry["directory"]
            report = Path(body["report"])
            assert report.stat().st_size > 0 and report.name == "report.md"
            assert list(report.parent.iterdir()) == [report]
            body = {"batch_dir":str(batch_dir), "batch":body["batch"], "run_dir":str(child), "result":json.loads((child/"result.json").read_text())}
            for name in ["batch.json", "seal.json"]:
                assert (batch_dir/name).is_file(), name
            subprocess.run([str(cli),"report",str(batch_dir)],stdout=subprocess.DEVNULL,check=True)
        row = dict(label=label, exit=code, seconds=round(time.monotonic()-started, 3))
        if "result" in body:
            progress = (work / (label + ".stderr")).read_text()
            record = body["result"]
            assert "C0 [1/1]" in progress and "REPORT  " in progress, label
            state = "PASSED" if record["status"] == "succeeded" else record["status"].replace("_", " ").upper()
            assert f"C0 {state}" in progress, label
            assert "[VALIDATE" not in progress and "[MEASURE" not in progress, label
            assert not any(line.startswith("  LIVE") and "/rq/" in line for line in progress.splitlines()), label
            if record["status"] == "succeeded":
                assert "  ERROR " not in progress and "  FATAL " not in progress, label
                assert f"DONE  Correctness: {len(record['validation'])}/{len(record['validation'])} checks" in progress, label
                assert f"DONE  Performance: {len(record['samples'])}/{len(record['samples'])} samples" in progress, label
                assert f"DONE  Diagnostics: {len(record['diagnostics'])}/{len(record['diagnostics'])} observations" in progress, label
                assert sum(line.startswith("  RUN   ") for line in progress.splitlines()) == 4, label
            if any(event.get("severity") == "fatal" and event.get("status") == "failed" for event in record["validation"]):
                assert "  FATAL " in progress, label
            if record["status"] in ["failed", "interrupted"]:
                assert record["error"] and f"Reason: {record['error'].split('; see ', 1)[0].splitlines()[0][:80]}" in progress, label
            row.update(run=body["run_dir"], status=body["result"]["status"], stage=body["result"]["stage"], error=body["result"].get("error"))
        ledger.append(row)
        (work / "ledger.json").write_text(json.dumps(ledger, indent=2)+"\n")
        print(json.dumps(row), flush=True)
        if expected is not None:
            assert code == expected, row
        return body, code

    def run(label, engine, operator, scheme="rq", action="run", expected=None, interrupt=False, test="0.1.0"):
        config = work / (label + ".toml")
        config.write_text((f"engine0={json.dumps(engine)}\n" if engine else "") + (f"operator0={json.dumps(operator)}\n" if operator else "") + f"scheme={json.dumps(scheme)}\ntest={json.dumps(test)}\n[run]\ncache={json.dumps(str(cache))}\noutput={json.dumps(str(runs))}\n")
        body, code = command(label, [action, "--config", config], expected, interrupt)
        if "result" in body: attempts.append(body)
        if body.get("result", {}).get("status") == "succeeded":
            record = body["result"]
            assert code == 0 and not record["correctness"]["fatal_failures"]
            assert len(record["samples"]) == len(record["recipe"]["cases"]) * record["recipe"]["repetitions"]
            assert all(c["validation"]["status"] != "not_executed" for c in record["coverage"]["cases"])
            for name in ["samples.json", "diagnostics.json", "seal.json"]:
                assert (Path(body["run_dir"]) / name).is_file(), name
            command(label+"-report", ["report", body["run_dir"]], 0)
            markdown = (work / (label + "-report.stdout")).read_text()
            assert "## 2. Correctness" in markdown and "## 6. Coverage" in markdown
            results[label] = body
        elif body.get("result", {}).get("status") != "prepared":
            assert not body.get("result", {}).get("summary"), "Failure published a complete performance summary"
        return body, code

    def known_operator(label, engine, operator, scheme):
        body, code = run(label, engine, operator, scheme)
        if code == 0:
            return
        record = body["result"]
        failures = record["correctness"]["fatal_failures"]
        assert code == 4 and record["stage"] == "validate", ledger[-1]
        assert any("MulAdd" in v.get("message", "") and "scalar: 0" in v.get("message", "") for v in failures), failures
        ledger[-1]["classification"] = "Verified fatal GF mul-add-zero defect; later cases remain not executed; no positive whole-suite coverage"
        (work / "ledger.json").write_text(json.dumps(ledger, indent=2)+"\n")

    for scheme in ["rq", "r10"]:
        run("public-engine-only-"+scheme, ENGINE, None, scheme, expected=0)

    if args.expect_upstream_x86_failure:
        import platform
        assert platform.machine() in ["x86_64", "AMD64"]
        for op in ["slab", "simd"]:
            body, _ = run("x86-"+op, ENGINE, OPERATORS+"#"+op, expected=4)
            assert body["result"]["stage"] == "build"
            log = (Path(body["run_dir"])/"logs/driver-build.stderr").read_text()
            assert "E0433" in log and "portable" in log
        print("Verified pinned upstream x86 operator build rejection; engine-only planning covered separately", flush=True)
        return

    arena = f"path:{args.arena.resolve()}" if args.arena else None
    fallback = arena or OPERATORS+"#slab"
    if args.reproduction and not arena:raise SystemExit("--reproduction requires an authorized --arena checkout")
    for scheme in ["rq", "r10"]:
        for op in ["slab", "simd"]:
            known_operator("public-"+op+"-"+scheme, ENGINE, OPERATORS+"#"+op, scheme)
        if arena:run("public-arena-"+scheme, ENGINE, arena, scheme, expected=0)
    if args.monorepo:
        mono = args.monorepo.resolve()
        for scheme in ["rq", "r10"]:
            for op in ["slab", "simd"]:
                known_operator("dev-"+op+"-"+scheme, f"path:{mono/'fountain_engine'}", f"path:{mono/'fountain_operators'}#{op}", scheme)
            for op in ["slab", "simd", *(["arena"] if arena else [])]:
                run("product-"+op+"-"+scheme, f"path:{mono/'fountain_bin_ffi'}", arena if op=="arena" else f"path:{mono/'fountain_operators'}#{op}", scheme, expected=0)
        if arena:
            body, _ = run("dev-arena-conflict", f"path:{mono/'fountain_engine'}", arena, expected=4)
            assert "conflict" in body["result"]["error"]
    for scheme in ["rq", "r10"]:
        matching = [v for key, v in results.items() if key.endswith("-"+scheme)]
        for index, other in enumerate(matching):
            common = set(matching[0]["result"]["recipe"]["cases"]) & set(other["result"]["recipe"]["cases"])
            command(f"compare-{scheme}-{index}", ["compare", matching[0]["run_dir"], other["run_dir"]], 0 if common else 2)
            if not common:
                assert "no common measured cases" in (work/f"compare-{scheme}-{index}.stderr").read_text()
    if arena:command("different-scheme", ["compare", results["public-arena-rq"]["run_dir"], results["public-arena-r10"]["run_dir"]], 2)

    if args.reproduction:
        # The frozen run is independent of later changes to a local component.
        first=results["public-arena-rq"]
        source=json.loads((Path(first["run_dir"])/"sources.json").read_text())["operator"]
        checkout=work/"generated-fixtures/arena"
        shutil.copytree(source["source"]["root"],checkout)
        subprocess.run(["git","-C",str(checkout),"init","--quiet"],check=True)
        prepared,_=run("snapshot",ENGINE,f"path:{checkout}",action="prepare",expected=0)
        lib=checkout/"src/lib.rs"
        lib.write_text('compile_error!("changed original after preparation");\n'+lib.read_text())
        repeated,_=command("prepared-whole-run",["run","--prepared",prepared["batch_dir"]],0)
        command("prepared-compare",["compare",first["run_dir"],repeated["run_dir"]],0)
        failed,_=run("changed-input",ENGINE,f"path:{checkout}",expected=4)
        assert failed["result"]["stage"]=="build"
        binary=Path(prepared["result"]["built"]["binary"])
        with binary.open("ab") as out:out.write(b"corruption fixture")
        corrupt,_=command("corrupt-cache",["run","--prepared",prepared["batch_dir"]],4)
        assert "checksum" in corrupt["result"]["error"]

    if args.faults:
        def fixture(name, injection):
            root=work/"generated-fixtures"/name
            (root/"src").mkdir(parents=True)
            (root/"Cargo.toml").write_text('[package]\nname="fountain-arena-operator"\nversion="0.2.0"\nedition="2024"\n[dependencies]\nfountain_engine="=2.1.0"\n')
            (root/"src/lib.rs").write_text('use fountain_engine::{traits::DataOperator,types::Operation};pub struct MemoryDataOperator{v:Vec<u8>}impl MemoryDataOperator{pub fn new(n:usize)->Self{'+injection+'Self{v:vec![0;n]}}}impl DataOperator for MemoryDataOperator{fn get_vector(&self,_:usize)->&[u8]{&self.v}fn execute(&mut self,_:&Operation){}}')
            subprocess.run(["git","-C",str(root),"init","--quiet"],check=True)
            return "path:"+str(root)
        for name, injection in [("wrong-output", ""), ("crash", "std::process::exit(23);"), ("protocol", 'println!("invalid event");std::process::exit(0);')]:
            body,_=run(name,ENGINE,fixture(name,injection),expected=4)
            assert body["result"]["stage"]=="validate"
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

    registered=set(attempts[0]["result"]["recipe"]["checks"])
    planned=[body["result"] for body in attempts if body["result"].get("built") and body["result"].get("recipe")]
    applicable={case for r in planned for case,c in r["recipe"]["checks"].items() if c["skip"] is None}
    reached={v["case"] for body in attempts for v in body["result"]["validation"]}
    positive={v["case"] for body in attempts for v in body["result"]["validation"] if v["status"]=="passed" or v["severity"]=="warning"}
    (work/"reachability.json").write_text(json.dumps({"registered":len(registered),"reached":len(reached),"passed_or_warning":len(positive),"applicable_not_executed":sorted(applicable-reached),"applicable_without_positive_evidence":sorted(applicable-positive),"inapplicable_for_all_selected_integrations":sorted(registered-applicable),"note":"Fatal-blocked cases are coverage gaps, not passes; outside-domain rows remain explicit applicability exclusions"},indent=2)+"\n")
    print(f"Framework acceptance finished; inspect ledger and reachability for component failures and coverage gaps: {work}",flush=True)


if __name__=="__main__":
    main()
