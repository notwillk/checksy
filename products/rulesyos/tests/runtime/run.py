#!/usr/bin/env python3
"""Run KVM-only RulesyOS boot checks with Buildroot's runtime-test emulator."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import sys
import tempfile

import pexpect


def kvm_is_ready() -> bool:
    if os.uname().machine != "x86_64":
        return False

    try:
        if not any(
            line.split() == ["232", "kvm"]
            for line in Path("/proc/misc").read_text().splitlines()
        ):
            return False
        device = os.stat("/dev/kvm")
    except (FileNotFoundError, PermissionError):
        return False

    return (
        stat.S_ISCHR(device.st_mode)
        and os.major(device.st_rdev) == 10
        and os.minor(device.st_rdev) == 232
        and os.access("/dev/kvm", os.R_OK | os.W_OK)
    )


def load_emulator(buildroot: Path):
    testing_dir = buildroot / "support" / "testing"
    if not testing_dir.is_dir():
        raise SystemExit(f"Buildroot runtime-test framework not found at {testing_dir}")
    sys.path.insert(0, str(testing_dir))

    from infra.emulator import Emulator

    return Emulator


def create_emulator(Emulator, buildroot: Path, name: str, download_dir: Path):
    log_base = buildroot.parent.parent / "runtime" / name
    log_base.parent.mkdir(parents=True, exist_ok=True)
    Path(f"{log_base}-run.log").unlink(missing_ok=True)
    emulator = Emulator(
        str(log_base),
        str(download_dir),
        True,
        1,
    )
    return emulator, log_base


def stop_on_signal(signum, _frame):
    raise SystemExit(128 + signum)


def boot_known_good(args: argparse.Namespace) -> None:
    Emulator = load_emulator(args.buildroot)
    emulator, log_base = create_emulator(
        Emulator, args.buildroot, "known-good", args.disk.parent
    )
    try:
        emulator.boot(
            "x86_64",
            kernel=str(args.kernel),
            kernel_cmdline=[
                "console=ttyS0",
                "root=/dev/vda1",
                "ro",
                "dslist=none",
            ],
            options=[
                "-accel",
                "kvm",
                "-machine",
                "pc",
                "-cpu",
                "host",
                "-monitor",
                "none",
                "-no-reboot",
                "-nic",
                "user,model=virtio-net-pci",
                "-initrd",
                str(args.initramfs),
                "-snapshot",
                "-drive",
                f"file={args.disk},if=virtio,format=qcow2",
            ],
        )
        emulator.qemu.expect("login as 'cirros' user", timeout=120)
    except Exception:
        print(f"CirrOS serial log: {log_base}-run.log", file=sys.stderr)
        raise
    finally:
        emulator.stop()

    print("Pinned CirrOS reached its serial login marker with KVM.")


EXPECTED_STATUS = {
    "schemaVersion": 1,
    "rulesyosVersion": "0.1.0",
    "firmwareSlot": None,
    "firmwareHealthy": True,
    "sourceKind": "baked",
    "candidateDigest": None,
    "rulesyVersion": "0.8.3",
    "rulesyDigest": "sha256:8ad81a0bbfde0d212c9efc1280a27c06bb00fb91cf7a24811729c931c0f50176",
    "rulesyExit": 0,
    "rulesySignal": None,
    "outcome": "converged",
}
STATUS_PATTERN = re.compile(r"RULESYOS_STATUS (\{[^\r\n]+\})")
BAKED_RULE_NAME = "Apply the baked RulesyOS configuration"
BAKED_FIX_LINE = f"{BAKED_RULE_NAME} fix"


def validate_status(status: dict) -> None:
    expected_fields = set(EXPECTED_STATUS) | {
        "bootId",
        "configDigest",
        "durationMs",
    }
    if set(status) != expected_fields:
        raise RuntimeError(
            "RulesyOS status fields were "
            f"{sorted(status)}, expected {sorted(expected_fields)}"
        )

    for field, expected in EXPECTED_STATUS.items():
        actual = status.get(field)
        if actual != expected:
            raise RuntimeError(
                f"RulesyOS status field {field!r} was {actual!r}, expected {expected!r}"
            )

    boot_id = status.get("bootId")
    if not isinstance(boot_id, str) or not boot_id:
        raise RuntimeError("RulesyOS status bootId must be a nonempty string")

    config_digest = status.get("configDigest")
    if not isinstance(config_digest, str) or not re.fullmatch(
        r"sha256:[0-9a-f]{64}", config_digest
    ):
        raise RuntimeError(
            "RulesyOS status configDigest must be a lowercase sha256:<hex> digest"
        )

    duration = status.get("durationMs")
    if not isinstance(duration, int) or isinstance(duration, bool) or duration < 0:
        raise RuntimeError("RulesyOS status durationMs must be a nonnegative integer")


def boot_built_once(
    Emulator, args: argparse.Namespace, disk: Path, boot_name: str, expect_fix: bool
) -> tuple[dict, Path]:
    emulator, log_base = create_emulator(
        Emulator, args.buildroot, boot_name, disk.parent
    )
    log_path = Path(f"{log_base}-run.log")
    try:
        emulator.boot(
            "x86_64",
            kernel=str(args.kernel),
            kernel_cmdline=[
                "rootwait",
                "root=/dev/vda1",
                "ro",
                "console=ttyS0",
            ],
            options=[
                "-accel",
                "kvm",
                "-machine",
                "pc",
                "-cpu",
                "host",
                "-monitor",
                "none",
                "-no-reboot",
                "-nic",
                "none",
                "-drive",
                f"file={disk},if=virtio,format=raw",
            ],
        )

        index = emulator.qemu.expect(
            [
                STATUS_PATTERN,
                r"login:",
            ],
            timeout=120,
        )
        if index == 1:
            raise RuntimeError("RulesyOS exposed a login prompt")

        status = json.loads(emulator.qemu.match.group(1))
        validate_status(status)
        if emulator.qemu.expect([r"login:", pexpect.TIMEOUT], timeout=2) == 0:
            raise RuntimeError("RulesyOS exposed a login prompt")
    except Exception:
        print(f"RulesyOS serial log: {log_path}", file=sys.stderr)
        raise
    finally:
        emulator.stop()
        emulator.logfile.flush()

    serial_log = log_path.read_text(errors="replace")
    if "login:" in serial_log:
        raise RuntimeError(f"RulesyOS exposed a login prompt; see {log_path}")
    if BAKED_RULE_NAME not in serial_log:
        raise RuntimeError(f"Rulesy did not report the baked rule; see {log_path}")
    fix_ran = BAKED_FIX_LINE in serial_log
    if expect_fix and not fix_ran:
        raise RuntimeError(f"first RulesyOS boot did not apply the baked fix; see {log_path}")
    if not expect_fix and fix_ran:
        raise RuntimeError(f"second RulesyOS boot repeated the baked fix; see {log_path}")
    if serial_log.count("RULESYOS_STATUS ") != 1:
        raise RuntimeError(
            f"RulesyOS emitted an unexpected number of completion statuses; see {log_path}"
        )

    return status, log_path


def boot_built(args: argparse.Namespace) -> None:
    Emulator = load_emulator(args.buildroot)
    with tempfile.TemporaryDirectory(
        prefix="rulesyos-runtime-", dir=args.disk.parent
    ) as temporary_dir:
        disk = Path(temporary_dir) / "rulesyos.img"
        shutil.copyfile(args.disk, disk)

        first, first_log = boot_built_once(
            Emulator, args, disk, "built-first", expect_fix=True
        )
        second, second_log = boot_built_once(
            Emulator, args, disk, "built-second", expect_fix=False
        )

    if first["bootId"] == second["bootId"]:
        raise RuntimeError("RulesyOS reused its boot ID across two boots")
    if first["configDigest"] != second["configDigest"]:
        raise RuntimeError("RulesyOS changed its baked configuration digest across boots")

    print(
        "RulesyOS converged idempotently across two KVM boots "
        f"({first_log}, {second_log})."
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--buildroot", type=Path, required=True)
    subparsers = parser.add_subparsers(dest="test", required=True)
    known_good = subparsers.add_parser("known-good")
    known_good.add_argument("--disk", type=Path, required=True)
    known_good.add_argument("--kernel", type=Path, required=True)
    known_good.add_argument("--initramfs", type=Path, required=True)
    built = subparsers.add_parser("built")
    built.add_argument("--kernel", type=Path, required=True)
    built.add_argument("--disk", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    if not kvm_is_ready():
        print(
            "WARNING: KVM acceleration is unavailable; skipping RulesyOS VM test.",
            file=sys.stderr,
        )
        return 0

    signal.signal(signal.SIGINT, stop_on_signal)
    signal.signal(signal.SIGTERM, stop_on_signal)

    args = parse_args()
    if args.test == "known-good":
        boot_known_good(args)
    elif args.test == "built":
        boot_built(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
