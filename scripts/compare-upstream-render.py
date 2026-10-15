#!/usr/bin/env python3
"""Compare actual no-profile 16-bit TIFF exports from two desktop executables."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--upstream-binary', type=Path, required=True)
p.add_argument('--integrated-binary', type=Path, required=True)
p.add_argument('--fixture-dir', type=Path, required=True)
p.add_argument('--output-dir', type=Path, required=True)
a = p.parse_args()
for path in (a.upstream_binary, a.integrated_binary,
             a.fixture_dir / 'synthetic-camera.dng', a.fixture_dir / 'neutral.json'):
    if not path.is_file():
        p.error(f'missing required input: {path}')
if a.output_dir.exists():
    p.error('output-dir must be new, so stale outputs cannot pass this check')
a.output_dir.mkdir(parents=True)
receipt = {}
for name, binary in [('upstream', a.upstream_binary), ('integrated', a.integrated_binary)]:
    output = (a.output_dir / f'{name}.tif').resolve()
    result = subprocess.run([
        str(binary.resolve()), 'export', str((a.fixture_dir / 'synthetic-camera.dng').resolve()),
        '--output', str(output), '--format', 'tiff', '--tiff-bit-depth', '16',
        '--adjustments', str((a.fixture_dir / 'neutral.json').resolve()),
    ], capture_output=True, text=True, timeout=180)
    (a.output_dir / f'{name}.log').write_text(result.stdout + result.stderr)
    if result.returncode != 0 or not output.is_file() or output.stat().st_size < 8:
        raise SystemExit(f'{name} export failed or produced no output; inspect its log')
    receipt[name] = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                     'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest(),
                     'output_bytes': output.stat().st_size}
receipt['byte_identical'] = receipt['upstream']['output_sha256'] == receipt['integrated']['output_sha256']
(a.output_dir / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
if not receipt['byte_identical']:
    raise SystemExit('FAIL: exports differ; inspect pixels and logs before accepting upstream preservation')
print('PASS: actual desktop no-profile exports are byte-identical on this fixture')
