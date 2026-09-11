"""Score each mutation on two kill criteria, over the mutations that score.

  artifact kill  -- does `replay conformance` produce different bytes?
                    What the conformance contract would reject.
  assertion kill -- does `cargo test` fail?
                    What the checked-in assertions name.

Three stages, and only the third is a score:

  1. the mutation compiles
  2. it changes behaviour
  3. something detects the change

Stage 2 is why the denominator is not simply "mutations written". A mutation
that changes no behaviour cannot be detected by anything, and counting it as a
survivor understates the suite. One entry here was a comment on a struct field
and read as a gap until it was looked at.

Stage 2 is mechanised in one direction only, and the asymmetry matters. The
release binary hashes stably across rebuilds of identical source, so a mutation
whose binary is unchanged provably changed nothing and is reported as
equivalent rather than counted. The converse does not hold: a different binary
is not proof of different behaviour, because codegen can move without meaning
moving. So this catches the class it catches, and a survivor still deserves a
look before it is believed.
"""
import hashlib, json, pathlib, subprocess, sys, time

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from mutants import M

ROOT = pathlib.Path(__file__).resolve().parent.parent
BIN = ROOT / "target/release/otransit"
OUT = pathlib.Path(__file__).parent / "results.json"


def sh(*a):
    return subprocess.run(a, cwd=ROOT, capture_output=True, text=True)


def build():
    """Build, and say why not when it fails.

    A toolchain that is broken for its own reasons reports `compile-fail` for
    every mutation in turn, which reads as a list of bad mutations. The first
    line of the error says which it is.
    """
    r = sh("cargo", "build", "--release")
    if r.returncode != 0:
        first = next((l for l in r.stderr.splitlines() if l.startswith("error")), "")
        return False, first
    return True, ""


def digest(b):
    return hashlib.sha256(b).hexdigest()[:12]


def binary():
    return digest(BIN.read_bytes())


def artifact():
    """The artifact's digest, or None when the replay refused to produce one.

    A mutation that makes `replay` exit non-zero is detectable, but it is not
    the contract rejecting a divergence -- it is the harness falling over. The
    two are reported apart, because a broken fixture reads as a kill otherwise
    and hides that nothing was actually compared.
    """
    r = sh(str(BIN), "replay", "conformance", "styles", "semantic")
    return digest(r.stdout.encode()) if r.returncode == 0 else None


def tests_pass():
    return sh("cargo", "test", "--release").returncode == 0


# A modified tree would become the baseline, and every score would be measured
# against a state no commit holds. The printed hashes give no hint of that.
#
# Tracked changes only. An untracked file is not compiled and does not reach
# `replay`, which reads its world from a fixture directory, so refusing on one
# would block the tool over a scratch file or a local settings directory.
dirty = sh("git", "status", "--porcelain", "--untracked-files=no").stdout.strip()
if dirty:
    print("the working tree is not clean, so the baseline is not a commit:\n")
    print(dirty)
    print("\ncommit or stash first, or pass --dirty to measure it anyway.")
    if "--dirty" not in sys.argv:
        raise SystemExit(1)

head = sh("git", "rev-parse", "--short", "HEAD").stdout.strip()
ok, why = build()
if not ok:
    raise SystemExit(f"the baseline does not build: {why}")
base_bin, base_art = binary(), artifact()
assert base_art is not None, "the baseline does not replay"
assert tests_pass(), "baseline tests do not pass"
print(f"baseline {head}  binary {base_bin}  artifact {base_art}\n", flush=True)

SCORED = {"AA", "A.", ".A", ".."}   # the outcomes that mean a real mutation ran


def score():
    """One of six outcomes, and a note for the ones that are not a score.

    One field rather than a `valid` flag beside three booleans. The flag made
    two dict shapes, and the second had keys the first did not, so reading a
    result meant knowing which kind it was first.
    """
    ok, why = build()
    if not ok:
        return "--", f"  <- does not compile: {why}"
    if binary() == base_bin:
        return "--", "  <- equivalent: binary unchanged"
    art = artifact()
    if art is None:
        return "!!", "  <- replay refused to run, so nothing was compared"
    return ("A" if art != base_art else ".") + ("A" if not tests_pass() else "."), ""


results = []
for i, (cat, name, f, find, rep) in enumerate(M, 1):
    path = ROOT / f
    original = path.read_text()
    assert original.count(find) == 1, f"{name}: find string is not unique"
    t0 = time.time()
    # Restored whatever happens below. A run takes minutes, and an interrupt
    # between the write and the restore would leave a mutation in the tree.
    try:
        path.write_text(original.replace(find, rep, 1))
        outcome, note = score()
    finally:
        path.write_text(original)
    results.append(dict(cat=cat, name=name, file=f, outcome=outcome))
    print(f"{i:2}/{len(M)} [{outcome:2}] {cat:9} {name}{note}  ({time.time() - t0:.0f}s)", flush=True)

OUT.write_text(json.dumps(results, indent=1))
build()

scored = [r for r in results if r["outcome"] in SCORED]
crashed = [r for r in results if r["outcome"] == "!!"]
n = len(scored)
print(f"\nwritten {len(results)}, scored {n} (the denominator)")
if crashed:
    print(f"replay refused to run under {len(crashed)}: {[r['name'] for r in crashed]}")
    print("nothing was compared for those. Look at them before believing the rest.")
print(f"artifact kill  {sum(r['outcome'][0] == 'A' for r in scored)}/{n}")
print(f"assertion kill {sum(r['outcome'][1] == 'A' for r in scored)}/{n}")
print(f"survived both  {sum(r['outcome'] == '..' for r in scored)}/{n}")
print("\nArtifact kill is this suite's discrimination benchmark, comparable to")
print("its own history. It is not a coverage percentage: the mutations are a")
print("sample of mistakes their author could imagine, not of mistakes a port")
print("will make.")
