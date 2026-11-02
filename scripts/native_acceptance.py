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
ARENA = "git+https://github.com/Gideon-0709/fountain-arena-operator.git?package=fountain-arena-operator&rev=d777cad6b512ee17663e129512bae10bca3397b1"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=Path("target/debug/fountain-eval"))
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
    cli = work / "fountain-eval"
    shutil.copy2(args.cli.resolve(), cli)
    runs, cache = work / "runs", work / "cache"
    ledger, results = [], {}

    def command(label, arguments, expected=None, interrupt=False):
        started = time.monotonic()
        before = set(runs.glob("*"))
        with (work / (label + ".stdout")).open("w") as out, (work / (label + ".stderr")).open("w") as err:
            process = subprocess.Popen([str(cli), *map(str, arguments)], cwd=work, stdout=out, stderr=err)
            if interrupt:
                deadline = time.monotonic() + 180
                while process.poll() is None and time.monotonic() < deadline:
                    if any((p / "logs/validate.stdout").exists() for p in set(runs.glob("*")) - before):
                        process.send_signal(signal.SIGINT)
                        break
                    time.sleep(.05)
            code = process.wait(timeout=7200)
        text = (work / (label + ".stdout")).read_text()
        body = json.loads(text) if text.strip() else {}
        row = dict(label=label, exit=code, seconds=round(time.monotonic()-started, 3))
        if "result" in body:
            row.update(run=body["run_dir"], status=body["result"]["status"], stage=body["result"]["stage"], error=body["result"].get("error"))
        ledger.append(row)
        (work / "ledger.json").write_text(json.dumps(ledger, indent=2)+"\n")
        print(json.dumps(row), flush=True)
        if expected is not None:
            assert code == expected, row
        return body, code

    def run(label, engine, operator, scheme="rq", action="run", expected=None, interrupt=False, test="0.1.0"):
        config = work / (label + ".toml")
        config.write_text(f"engine={json.dumps(engine)}\noperator={json.dumps(operator)}\nscheme={json.dumps(scheme)}\ntest={json.dumps(test)}\n[run]\ncache={json.dumps(str(cache))}\noutput={json.dumps(str(runs))}\n")
        body, code = command(label, [action, "--config", config], expected, interrupt)
        if body.get("result", {}).get("status") == "succeeded":
            record = body["result"]
            assert code == 0 and not record["correctness"]["fatal_failures"]
            assert len(record["samples"]) == len(record["recipe"]["cases"]) * record["recipe"]["repetitions"]
            assert all(c["validation"]["status"] != "not_executed" for c in record["coverage"]["cases"])
            for name in ["report.html", "report.md", "samples.csv", "diagnostics.json", "seal.json"]:
                assert (Path(body["run_dir"]) / name).is_file(), name
            report, _ = command(label+"-report", ["report", body["run_dir"]], 0)
            assert report["coverage"] == record["coverage"]
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

    if args.expect_upstream_x86_failure:
        import platform
        assert platform.machine() in ["x86_64", "AMD64"]
        for op in ["slab", "simd"]:
            body, _ = run("x86-"+op, ENGINE, OPERATORS+"#"+op, expected=4)
            assert body["result"]["stage"] == "build"
            log = (Path(body["run_dir"])/"logs/driver-build.stderr").read_text()
            assert "E0433" in log and "portable" in log
        print("Verified pinned upstream x86 build rejection; no positive native coverage", flush=True)
        return

    arena = f"path:{args.arena.resolve()}" if args.arena else ARENA
    for scheme in ["rq", "r10"]:
        for op in ["slab", "simd"]:
            known_operator("public-"+op+"-"+scheme, ENGINE, OPERATORS+"#"+op, scheme)
        run("public-arena-"+scheme, ENGINE, arena, scheme, expected=0)
    if args.monorepo:
        mono = args.monorepo.resolve()
        for scheme in ["rq", "r10"]:
            for op in ["slab", "simd"]:
                known_operator("dev-"+op+"-"+scheme, f"path:{mono/'fountain_engine'}", f"path:{mono/'fountain_operators'}#{op}", scheme)
            for op in ["slab", "simd", "arena"]:
                run("product-"+op+"-"+scheme, f"path:{mono/'fountain_bin_ffi'}", arena if op=="arena" else f"path:{mono/'fountain_operators'}#{op}", scheme, expected=0)
        body, _ = run("dev-arena-conflict", f"path:{mono/'fountain_engine'}", arena, expected=4)
        assert "conflict" in body["result"]["error"]
    for scheme in ["rq", "r10"]:
        matching = [v for key, v in results.items() if key.endswith("-"+scheme)]
        for index, other in enumerate(matching):
            command(f"compare-{scheme}-{index}", ["compare", matching[0]["run_dir"], other["run_dir"]], 0)
    command("different-scheme", ["compare", results["public-arena-rq"]["run_dir"], results["public-arena-r10"]["run_dir"]], 2)

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
        repeated,_=command("prepared-whole-run",["run","--prepared",prepared["run_dir"]],0)
        command("prepared-compare",["compare",first["run_dir"],repeated["run_dir"]],0)
        failed,_=run("changed-input",ENGINE,f"path:{checkout}",expected=4)
        assert failed["result"]["stage"]=="build"
        binary=Path(prepared["result"]["built"]["binary"])
        with binary.open("ab") as out:out.write(b"corruption fixture")
        corrupt,_=command("corrupt-cache",["run","--prepared",prepared["run_dir"]],4)
        assert "checksum" in corrupt["result"]["error"]

    if args.faults:
        def fixture(name, injection):
            root=work/"generated-fixtures"/name
            (root/"src").mkdir(parents=True)
            (root/"Cargo.toml").write_text('[package]\nname="fountain-arena-operator"\nversion="0.2.0"\nedition="2024"\n[dependencies]\nfountain_engine="=2.1.0"\n')
            (root/"src/lib.rs").write_text('use fountain_engine::{traits::DataOperator,types::Operation};pub struct MemoryDataOperator{v:Vec<u8>}impl MemoryDataOperator{pub fn new(n:usize)->Self{'+injection+'Self{v:vec![0;n]}}}impl DataOperator for MemoryDataOperator{fn get_vector(&self,_:usize)->&[u8]{&self.v}fn execute(&mut self,_:&Operation){}}')
            subprocess.run(["git","-C",str(root),"init","--quiet"],check=True)
            return "path:"+str(root)
        for name, injection in [("wrong-output", ""), ("crash", "std::process::exit(23);"), ("protocol", 'println!("invalid event");')]:
            body,_=run(name,ENGINE,fixture(name,injection),expected=4)
            assert body["result"]["stage"]=="validate"
        hung=fixture("hang", "std::thread::sleep(std::time::Duration::from_secs(900));")
        body,_=run("interrupt",ENGINE,hung,expected=130,interrupt=True)
        assert body["result"]["status"]=="interrupted"
        # A short timeout and process-group cleanup are separately exercised by tests/processes.rs.
        run("missing-reference","path:/fountain-eval-nonexistent",arena,expected=4)
        run("unknown-test",ENGINE,arena,test="9.0.0",expected=3)
        run("unknown-scheme",ENGINE,arena,scheme="missing",expected=3)

    registered=set(next(iter(results.values()))["result"]["recipe"]["checks"])
    observed={v["case"] for body in results.values() for v in body["result"]["validation"]}
    (work/"reachability.json").write_text(json.dumps({"registered":len(registered),"exercised":len(observed),"unexecuted":sorted(registered-observed),"note":"Unexecuted cases are coverage gaps, including cases behind upstream fatal checks; they are not acceptance passes"},indent=2)+"\n")
    print(f"Framework acceptance finished; inspect ledger and reachability for component failures and coverage gaps: {work}",flush=True)


if __name__=="__main__":
    main()
