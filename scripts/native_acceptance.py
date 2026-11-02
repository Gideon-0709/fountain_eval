#!/usr/bin/env python3
"""Maintainer acceptance of the real CLI. Every attempt has a unique retained run.

No retries, uploads, performance thresholds, or automatic result deletion.
Optional private checkouts are read-only; fault fixtures live in the supplied work dir.
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


def composition_acceptance(work, source, run):
    """Exercise actual workspace expansion, features and an external path dependency."""
    workspace=work / "fixtures/inherited-workspace"
    operator=workspace / "operator"
    helper=work / "fixtures/external-helper"
    shutil.copytree(source,operator)
    (helper / "src").mkdir(parents=True)
    (helper / "Cargo.toml").write_text('[package]\nname="eval-acceptance-helper"\nversion="0.0.0"\nedition="2024"\n')
    (helper / "src/lib.rs").write_text('pub const VALUE:u8=7;\n')
    (workspace / "Cargo.toml").write_text('[workspace]\nmembers=["operator"]\nresolver="3"\n[workspace.package]\nversion="2.0.0"\nedition="2024"\n[workspace.dependencies]\nfountain_engine="2.1.0"\n[workspace.lints.rust]\nunused_imports="allow"\n')
    manifest=operator / "Cargo.toml"
    text=manifest.read_text().replace('version = "2.0.0"','version.workspace = true',1).replace('edition = "2024"','edition.workspace = true',1).replace('fountain_engine = "2.1.0"','fountain_engine.workspace = true',1)
    text=text.split('[lints.clippy]')[0]+'[lints]\nworkspace=true\n[dependencies.eval-acceptance-helper]\npath="../../external-helper"\nversion="=0.0.0"\n'
    manifest.write_text(text)
    lib=operator / "src/lib.rs"
    lib.write_text(lib.read_text()+'\nconst _:u8=eval_acceptance_helper::VALUE;\n')
    for root in [workspace,helper]:
        subprocess.run(["git","-C",str(root),"init","--quiet"],check=True)
    first=run("workspace-and-external-path",ENGINE,f"path:{operator}#slab","0.1.0")
    graph=first["result"]["built"]["dependency_graph"]["driver"]
    package=next(p for p in graph["packages"] if p["name"]=="fountain_operators")
    node=next(n for n in graph["resolve"]["nodes"] if n["id"]==package["id"])
    assert "simd" in node["features"]
    (helper / "src/lib.rs").write_text('pub const VALUE:u8=8;\n')
    second=run("external-path-change",ENGINE,f"path:{operator}#slab","0.1.0")
    assert first["result"]["built"]["key"]!=second["result"]["built"]["key"], "External input change reused a stale build"
    assert first["result"]["quadruple"]["c"]["operator"]["source_digest"]==second["result"]["quadruple"]["c"]["operator"]["source_digest"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=Path("target/debug/fountain-eval"))
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--monorepo", type=Path)
    parser.add_argument("--arena", type=Path)
    parser.add_argument("--faults", action="store_true")
    parser.add_argument("--expect-upstream-x86-failure", action="store_true", help="Verify the pinned upstream AVX2 compile failure; this is not positive native coverage")
    args = parser.parse_args()
    cli = args.cli.resolve()
    work = args.work.resolve()
    if work.exists():
        raise SystemExit("Use a new work directory; previous attempts are not overwritten.")
    work.mkdir(parents=True)
    frozen_cli=work / "fountain-eval"
    shutil.copy2(cli,frozen_cli)
    cli=frozen_cli
    cache, runs = work / "cache", work / "runs"
    ledger = []
    results = {}

    def command(label, arguments, expected=0, interrupt=False):
        cmd = [str(cli), *map(str, arguments)]
        start = time.monotonic()
        before = set(runs.glob("*"))
        process = subprocess.Popen(cmd, cwd=work, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        if interrupt:
            deadline = time.monotonic() + 45
            while True:
                new = set(runs.glob("*")) - before
                if any((p / "logs/validate.stdout").is_file() and (p / "logs/validate.stdout").stat().st_size for p in new):
                    process.send_signal(signal.SIGINT)
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    process.terminate()
                    raise RuntimeError("Interruption fixture did not reach validation")
                time.sleep(0.05)
        stdout, stderr = process.communicate(timeout=240)
        (work / f"{label}.stdout").write_text(stdout)
        (work / f"{label}.stderr").write_text(stderr)
        row = {"label": label, "exit": process.returncode, "expected_exit": expected, "seconds": round(time.monotonic() - start, 3)}
        try:
            body = json.loads(stdout)
        except json.JSONDecodeError:
            body = {}
        if "run_dir" in body:
            row.update(run=body["run_dir"], status=body["result"]["status"], stage=body["result"]["stage"], error=body["result"].get("error"))
        ledger.append(row)
        (work / "ledger.json").write_text(json.dumps(ledger, indent=2) + "\n")
        print(json.dumps(row), flush=True)
        if process.returncode != expected and "run" in row:
            logs=Path(row["run"]) / "logs"
            for log in sorted(logs.glob("*.stderr"), key=lambda p:p.stat().st_mtime)[-4:]:
                detail=log.read_text(errors="replace")
                if detail:
                    lines=detail.splitlines()
                    errors=["\n".join(lines[i:i+12]) for i,line in enumerate(lines) if line.startswith("error[") or line.startswith("error:")]
                    print(f"--- {log.name} ---\n"+"\n".join(errors)+"\n"+detail[-4000:],flush=True)
        assert process.returncode == expected, (row, stderr[-2000:])
        return body

    def run(label, engine, operator, test="0.1.0", expected=0, action="run", interrupt=False, scheme="rq", scheme_source=None):
        scheme_value=json.dumps(scheme) if scheme_source is None else "{ name = " + json.dumps(scheme) + ", source = " + json.dumps(scheme_source) + " }"
        config=work / f"{label}.toml"
        config.write_text(f"engine = {json.dumps(engine)}\noperator = {json.dumps(operator)}\nscheme = {scheme_value}\ntest = {json.dumps(test)}\n[run]\ncache = {json.dumps(str(cache))}\noutput = {json.dumps(str(runs))}\n")
        body = command(label, [action, "--config", config], expected, interrupt)
        if expected == 0:
            results[label] = body
            record = body["result"]
            assert record["status"] == ("prepared" if action == "prepare" else "succeeded")
            if action == "run":
                product=record["built"]["family"]=="product-abi"
                assert len(record["samples"]) == (12 if product else 66)
                assert len(record["validation"]) == (16 if product else 22)
                assert len(record["recipe"]["checks"])==26
                assert record["coverage"]["skipped_validation"]==(10 if product else 4)
                assert record["coverage"]["skipped_measurement"]==(22 if product else 4)
                assert all(s["case"].startswith("operator/") or s["case"].startswith(f"codec/{scheme}/") for s in record["samples"])
                assert record["quadruple"]["c"]["scheme"]["package"]=="fountain_scheme"
                checks=record["correctness"]
                assert checks["fatal_checks"]==(4 if product else 10)
                assert checks["warning_checks"]==12
                expected_warnings=0 if record["built"]["implementation"]=="arena" else 12
                assert len(checks["warnings"])==expected_warnings, checks
                assert checks["status"]==("passed_with_warnings" if expected_warnings else "passed")
                assert not checks["fatal_failures"]
                assert json.loads((Path(body["run_dir"])/"correctness.json").read_text())==checks
            report=command(label + "-report", ["report", body["run_dir"]])
            assert report["coverage"]==record["coverage"]
            assert (Path(body["run_dir"])/"config.toml").read_text()==config.read_text()
        else:
            assert not body["result"]["summary"], "A failed run must not publish an aggregate performance result"
        return body

    if args.expect_upstream_x86_failure:
        import platform
        assert platform.machine() in ["x86_64","AMD64"], "This expectation is specific to x86_64"
        for op in ["slab","simd"]:
            body=run(f"upstream-x86-{op}",ENGINE,OPERATORS+"#"+op,"0.1.0",4)
            assert body["result"]["stage"]=="build"
            detail=(Path(body["run_dir"])/"logs/driver-build.stderr").read_text()
            assert "E0433" in detail and "portable" in detail, detail[-4000:]
            lines=detail.splitlines()
            for i,line in enumerate(lines):
                if line.startswith("error["):print("\n".join(lines[i:i+12]),flush=True)
            ledger[-1]["classification"]="known upstream x86 AVX2 compile error; no positive native coverage"
            (work/"ledger.json").write_text(json.dumps(ledger,indent=2)+"\n")
        print("Pinned upstream x86 failures verified; native positive coverage remains unavailable on this architecture.",flush=True)
        return

    pairs = [("public-slab", ENGINE, OPERATORS + "#slab"),
             ("public-simd", ENGINE, OPERATORS + "#simd")]
    if args.arena:
        arena = args.arena.resolve()
        pairs.append(("public-arena", ENGINE, f"path:{arena}"))
    if args.monorepo:
        mono = args.monorepo.resolve()
        for op in ["slab", "simd"]:
            pairs.append((f"dev-{op}", f"path:{mono / 'fountain_engine'}", f"path:{mono / 'fountain_operators'}#{op}"))
    for label, engine, operator in pairs:
        for scheme in ["rq","r10"]:
            run(label + "-" + scheme, engine, operator, scheme=scheme)
    registered=set(next(iter(results.values()))["result"]["recipe"]["checks"])
    exercised={event["case"] for body in results.values() for event in body["result"]["validation"]}
    assert exercised==registered, {"unreachable_cases":sorted(registered-exercised),"unexpected_cases":sorted(exercised-registered)}
    if args.monorepo:
        for op in ["slab", "simd", *(["arena"] if args.arena else [])]:
            operator = f"path:{arena}" if op == "arena" else f"path:{mono / 'fountain_operators'}#{op}"
            for scheme in ["rq","r10"]:
                run(f"product-{op}-{scheme}", f"path:{mono / 'fountain_bin_ffi'}", operator, scheme=scheme)
        run("product-external-scheme-rejected", f"path:{mono / 'fountain_bin_ffi'}", f"path:{mono / 'fountain_operators'}#slab", expected=3, scheme_source=f"path:{mono / 'fountain_scheme'}")
        if args.arena:
            rejected = run("dev-arena-conflict", f"path:{mono / 'fountain_engine'}", f"path:{arena}", "0.1.0", 4)
            assert "conflict" in rejected["result"]["error"]
    for test in ["rq", "r10"]:
        labels = [k for k in results if k.endswith("-" + test)]
        for label in labels[1:]:
            command(f"compare-{test}-{label}", ["compare", results[labels[0]]["run_dir"], results[label]["run_dir"]])
    command("rejected-different-schemes", ["compare",results["public-slab-rq"]["run_dir"],results["public-slab-r10"]["run_dir"]],2)
    if args.arena:
        revision = subprocess.check_output(["git", "-C", str(arena), "rev-parse", "HEAD"], text=True).strip()
        url = subprocess.check_output(["git", "-C", str(arena), "remote", "get-url", "origin"], text=True).strip()
        run("pinned-git", ENGINE, f"git+{url}?package=fountain-arena-operator&rev={revision}", "0.1.0")

    # Copy a resolved package into an independent fixture checkout. Never edit upstream.
    source = json.loads((Path(results["public-slab-rq"]["run_dir"]) / "sources.json").read_text())["operator"]
    explicit=run("explicit-registry-scheme",ENGINE,OPERATORS+"#slab",scheme_source="registry:fountain_scheme@2.1.0")
    assert explicit["result"]["built"]["key"]==results["public-slab-rq"]["result"]["built"]["key"]
    scheme_source=json.loads((Path(explicit["run_dir"])/"sources.json").read_text())["scheme"]
    local_scheme=work / "fixtures/selected-scheme"
    shutil.copytree(scheme_source["source"]["root"],local_scheme)
    subprocess.run(["git","-C",str(local_scheme),"init","--quiet"],check=True)
    selected=run("prepare-explicit-scheme",ENGINE,OPERATORS+"#slab",action="prepare",scheme_source=f"path:{local_scheme}")
    scheme_lib=local_scheme / "src/lib.rs"
    scheme_lib.write_text('compile_error!("selected scheme changed after preparation");\n'+scheme_lib.read_text())
    (work / "prepare-explicit-scheme.toml").write_text("invalid changed configuration")
    replay=command("scheme-and-config-frozen",["run","--prepared",selected["run_dir"]])
    assert replay["result"]["status"]=="succeeded"
    assert replay["result"]["configuration"]==selected["result"]["configuration"]
    failed_scheme=run("changed-scheme-compile-failure",ENGINE,OPERATORS+"#slab",expected=4,scheme_source=f"path:{local_scheme}")
    assert failed_scheme["result"]["stage"]=="build"
    composition_acceptance(work, Path(source["source"]["root"]), run)
    snapshot = work / "fixtures/snapshot-operator"
    shutil.copytree(source["source"]["root"], snapshot)
    subprocess.run(["git", "-C", str(snapshot), "init", "--quiet"], check=True)
    operator = f"path:{snapshot}#slab"
    prepared = run("prepare-snapshot", ENGINE, operator, "0.1.0", action="prepare")
    lib = snapshot / "src/lib.rs"
    lib.write_text('compile_error!("source changed after preparation");\n' + lib.read_text())
    replay = command("prepared-replay", ["run", "--prepared", prepared["run_dir"]])
    assert replay["result"]["status"] == "succeeded"
    repeated = command("prepared-repeat", ["run", "--prepared", prepared["run_dir"]])
    command("compare-repeat", ["compare", replay["run_dir"], repeated["run_dir"]])
    failed = run("fresh-input-compile-failure", ENGINE, operator, "0.1.0", 4)
    assert failed["result"]["stage"] == "build"
    # Binary corruption is rejected, without substituting a new build or rerunning the test.
    binary = Path(prepared["result"]["built"]["binary"])
    with binary.open("ab") as stream:
        stream.write(b"cache-corruption-fixture")
    corrupt = command("corrupt-prepared-cache", ["run", "--prepared", prepared["run_dir"]], 4)
    assert "checksum" in corrupt["result"]["error"]
    command("rejected-incomplete-comparison", ["compare", replay["run_dir"], corrupt["run_dir"]], 2)

    if args.faults:
        import re
        noisy=work / "fixtures/noisy-operator"
        shutil.copytree(source["source"]["root"],noisy)
        subprocess.run(["git","-C",str(noisy),"init","--quiet"],check=True)
        path=noisy / "src/slab_data_operator.rs"
        changed,count=re.subn(r'(pub fn new\([^)]*\)\s*->\s*Self\s*\{)',r'\1 println!("not a protocol event");',path.read_text(),count=1)
        assert count==1
        path.write_text(changed)
        protocol=run("driver-protocol-error",ENGINE,f"path:{noisy}#slab","0.1.0",4)
        assert "driver protocol" in protocol["result"]["error"]
        run("registry-operator", ENGINE, "registry:fountain_operators@2.0.0#slab", "0.1.0")
        def fixture(name, body, version="0.2.0", package="fountain-arena-operator"):
            root = work / "fixtures" / name
            (root / "src").mkdir(parents=True)
            (root / "Cargo.toml").write_text(f'[package]\nname="{package}"\nversion="{version}"\nedition="2024"\n[dependencies]\nfountain_engine="=2.1.0"\n')
            (root / "src/lib.rs").write_text(body)
            subprocess.run(["git", "-C", str(root), "init", "--quiet"], check=True)
            return f"path:{root}"
        template = '''use fountain_engine::{traits::DataOperator,types::Operation};
        pub struct MemoryDataOperator { bytes:Vec<u8> }
        impl MemoryDataOperator {pub fn new(n:usize)->Self{Self{bytes:vec![0;n]}}}
        impl DataOperator for MemoryDataOperator {
          fn config_finite_field(&mut self,_:u16){}
          fn insert_vector(&mut self,_:&[u8],_:usize){}
          fn get_vector(&self,_:usize)->&[u8]{&self.bytes}
          fn execute(&mut self,_:&Operation){ BODY }
        }'''
        bad = fixture("wrong-operator", template.replace("BODY", ""))
        wrong = run("wrong-operator-result", ENGINE, bad, "0.1.0", 4)
        assert wrong["result"]["stage"] == "validate"
        assert wrong["result"]["correctness"]["status"]=="failed"
        assert wrong["result"]["correctness"]["fatal_failures"]
        assert not wrong["result"]["samples"]
        crashed = fixture("crash-operator", template.replace("BODY", "std::process::exit(23);"))
        run("driver-crash", ENGINE, crashed, "0.1.0", 4)
        hung = fixture("hang-operator", template.replace("BODY", "std::thread::sleep(std::time::Duration::from_secs(90));"))
        interrupted = run("driver-interruption", ENGINE, hung, "0.1.0", 130, interrupt=True)
        assert interrupted["result"]["status"] == "interrupted"
        timed = run("driver-timeout", ENGINE, hung, "0.1.0", 4)
        assert "timeout" in timed["result"]["error"]
        unknown = fixture("unknown-interface", "", "4.0.0", "fountain_engine")
        run("unknown-interface", unknown, bad, "0.1.0", 3)
        run("unknown-scheme", ENGINE, bad, expected=3, scheme="missing")
        run("unknown-test", ENGINE, bad, "9.0.0", 3)
        run("invalid-local-reference", "path:/fountain-eval-fixture-does-not-exist", bad, "0.1.0", 4)
    print(f"Acceptance passed: {len(ledger)} checked commands. Evidence: {work}", flush=True)


if __name__ == "__main__":
    main()
