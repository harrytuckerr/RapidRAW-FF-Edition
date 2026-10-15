#!/usr/bin/env python3
"""Prepare, resolve and build a disposable Android copy; never install on devices."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import shlex
import subprocess
import sys

PINS = {
    'tauri-plugin-http': '2.7.0', 'tauri-plugin-fs': '2.5.2',
    'tauri-plugin-store': '2.4.0', 'tauri': '2.11.5',
    'tauri-runtime-wry': '2.11.4', 'tauri-runtime': '2.11.3',
    'wry': '0.55.1', 'tao': '0.35.3', 'tauri-build': '2.6.3',
    'tauri-macros': '2.6.3', 'tauri-plugin': '2.6.3',
    'tauri-codegen': '2.6.3', 'tauri-utils': '2.9.3',
}
LOCKS = {'Cargo.lock': 'src-tauri/Cargo.lock', 'package-lock.json': 'package-lock.json'}
MARKER = '.android-workspace.json'


def run(args, cwd, **kwargs):
    subprocess.run(args, cwd=cwd, check=True, **kwargs)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_pins(root):
    # Rust 1.98 hosts may have Python 3.10, so avoid requiring tomllib.
    import re
    packages = re.findall(r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"',
                          (root / 'src-tauri/Cargo.lock').read_text())
    for name, version in PINS.items():
        if [v for n, v in packages if n == name] != [version]:
            raise SystemExit(f'Android lock must pin {name} to {version}')
    packages = json.loads((root / 'package-lock.json').read_text())['packages']
    for name, version in {'cli': '2.11.4', 'api': '2.11.1', 'plugin-http': '2.7.0'}.items():
        if packages[f'node_modules/@tauri-apps/{name}']['version'] != version:
            raise SystemExit(f'Android npm lock must pin {name} to {version}')


def require_disk_budget(path):
    free = shutil.disk_usage(path).free
    if free < 6 * 1024**3:
        raise SystemExit(f'Paused: only {free / 1024**3:.2f} GiB free; at least 6 GiB required')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['prepare', 'resolve', 'build'])
    parser.add_argument('--workspace', type=Path, required=True)
    parser.add_argument('--source', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--locks', type=Path, help='Prepared Android lock bundle; required for prepare')
    parser.add_argument('--build-args', default='', help='Additional Tauri Android build options')
    parser.add_argument('--release', action='store_true', help='Build release instead of debug')
    args = parser.parse_args()
    source, work = args.source.resolve(), args.workspace.resolve()
    if source == work or source in work.parents or work in source.parents:
        raise SystemExit('Workspace must be outside the source tree and must not contain it')
    if args.command in ('prepare', 'resolve'):
        if work.exists():
            raise SystemExit('Workspace must not exist; use a fresh destination')
        conflicts = subprocess.check_output(['git', 'diff', '--name-only', '--diff-filter=U'], cwd=source)
        if conflicts.strip():
            raise SystemExit('Resolve source merge conflicts before Android preparation')
        if args.command == 'prepare' and not args.locks:
            raise SystemExit('prepare requires --locks; resolve explicitly bootstraps a new lock bundle')
        # Capture tracked working files plus deliberate untracked source, never symlink
        # shared node_modules/targets into this disposable workspace.
        files = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=source)
        source_files = {}
        for name in sorted(set(files.decode().split('\0')) - {''}):
            path = source / name
            if not path.is_file():
                continue
            if path.is_symlink():
                raise SystemExit(f'Refusing source symlink: {name}')
            dest = work / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, dest)
            copied_hash = digest(dest)
            if copied_hash != digest(path):
                raise SystemExit(f'Source changed during copy: {name}; use a fresh workspace')
            source_files[name] = {'sha256': copied_hash, 'bytes': dest.stat().st_size}
        # Catch concurrent edits during the copy, before any resolution starts.
        for name, entry in source_files.items():
            if not (source / name).is_file() or digest(source / name) != entry['sha256']:
                raise SystemExit(f'Source changed during copy: {name}; use a fresh workspace')
        package = json.loads((work / 'package.json').read_text())
        package['dependencies']['@tauri-apps/api'] = '2.11.1'
        package['dependencies']['@tauri-apps/plugin-http'] = '2.7.0'
        package['devDependencies']['@tauri-apps/cli'] = '2.11.4'
        (work / 'package.json').write_text(json.dumps(package, indent=2) + '\n')
        receipt = {
            'schema': 2,
            'source': str(source),
            'source_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source).decode().strip(),
            'source_files': source_files,
            'source_manifest_sha256': hashlib.sha256(
                json.dumps(source_files, sort_keys=True, separators=(',', ':')).encode()
            ).hexdigest(),
            'source_locks': {p: source_files[p]['sha256'] for p in LOCKS.values()},
        }
        if args.command == 'resolve':
            for name, version in PINS.items():
                require_disk_budget(work)
                run(['cargo', 'update', '-p', name, '--precise', version], work / 'src-tauri')
            require_disk_budget(work)
            run(['npm', 'install', '--package-lock-only', '--ignore-scripts'], work)
        else:
            for name, target in LOCKS.items():
                shutil.copy2(args.locks / name, work / target)
        check_pins(work)
        # Export both locks for review/reuse, never overwrite desktop locks.
        bundle = work / 'android-locks'
        bundle.mkdir()
        for name, target in LOCKS.items():
            shutil.copy2(work / target, bundle / name)
        receipt['prepared_files'] = {name: digest(work / name) for name in source_files}
        receipt['android_locks'] = {p: digest(work / p) for p in LOCKS.values()}
        (work / MARKER).write_text(json.dumps(receipt, indent=2) + '\n')
        print(f'Prepared {work}; frozen lock bundle: {bundle}')
        return
    require_disk_budget(work)
    receipt = json.loads((work / MARKER).read_text())
    if receipt.get('schema') != 2:
        raise SystemExit('Prepare a fresh workspace with a full source receipt')
    for path, expected in receipt['prepared_files'].items():
        if not (work / path).is_file() or digest(work / path) != expected:
            raise SystemExit(f'Prepared source changed: {path}; prepare a fresh source receipt')
    for path, expected in receipt['android_locks'].items():
        if digest(work / path) != expected:
            raise SystemExit(f'Prepared lock changed: {path}')
    check_pins(work)
    sdk = Path(os.environ.get('ANDROID_HOME', os.environ.get('ANDROID_SDK_ROOT', '/missing-sdk')))
    ndk = Path(os.environ.get('NDK_HOME', str(sdk / 'ndk/26.3.11579264')))
    if not (ndk / 'source.properties').is_file() or '26.3.11579264' not in (ndk / 'source.properties').read_text():
        raise SystemExit('NDK r26d (26.3.11579264) required; set NDK_HOME')
    for path in ('platforms/android-36/android.jar', 'build-tools/36.0.0'):
        if not (sdk / path).exists():
            raise SystemExit(f'Missing SDK component: {path}')
    java_home = os.environ.get('JAVA_HOME')
    if not java_home:
        raise SystemExit('Set JAVA_HOME to a JDK 17 installation')
    java_version = subprocess.check_output([str(Path(java_home) / 'bin/java'), '-version'], stderr=subprocess.STDOUT).decode()
    if 'version "17.' not in java_version:
        raise SystemExit('JDK 17 required; current JAVA_HOME selects a different version')
    toolchain = re.search(r'^channel\s*=\s*"([^"]+)"',
                          (work / 'src-tauri/rust-toolchain.toml').read_text(), re.MULTILINE)
    if not toolchain:
        raise SystemExit('Cannot determine the declared Rust toolchain')
    env = dict(os.environ, ANDROID_HOME=str(sdk), NDK_HOME=str(ndk),
               RUSTUP_TOOLCHAIN=toolchain.group(1),
               CARGO_TARGET_DIR=str(work / 'android-target'),
               CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_DEV_INCREMENTAL='false',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_PROFILE_TEST_INCREMENTAL='false',
               CARGO_INCREMENTAL='0',
               ORT_SKIP_DOWNLOAD='1', ORT_STRATEGY='manual',
               ORT_LIB_LOCATION=str(work / 'src-tauri/libs/arm64-v8a'))
    # npm ci refuses manifest/lock mismatches; cargo --locked forbids re-resolution.
    run(['cargo', 'metadata', '--locked', '--format-version', '1'],
        work / 'src-tauri', env=env, stdout=subprocess.DEVNULL)
    require_disk_budget(work)
    run(['npm', 'ci'], work, env=env)
    command = [str(work / 'node_modules/.bin/tauri'), 'android', 'build', '--target', 'aarch64', '--apk']
    if not args.release:
        command.append('--debug')
    extra = shlex.split(args.build_args)
    if '--' in extra:
        raise SystemExit('Pass only Tauri options in --build-args, not Cargo arguments')
    # Existing CI callers repeat the target; normalize it and reject other ABIs.
    for option in extra[:]:
        if option.startswith('--target='):
            if option != '--target=aarch64':
                raise SystemExit('Only aarch64 is supported')
            extra.remove(option)
    if '--target' in extra:
        index = extra.index('--target')
        if extra[index + 1:index + 2] != ['aarch64']:
            raise SystemExit('Only aarch64 is supported by the ONNX packaging in build.rs')
        del extra[index:index + 2]
    if '--debug' in extra and '--debug' in command:
        extra.remove('--debug')
    require_disk_budget(work)
    run(command + extra + ['--', '--locked'], work, env=env)
    for path, expected in receipt['android_locks'].items():
        if digest(work / path) != expected:
            raise SystemExit(f'Build modified prepared lock: {path}')
    print(f'APK output: {work}/src-tauri/gen/android/app/build/outputs/apk')


if __name__ == '__main__':
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
