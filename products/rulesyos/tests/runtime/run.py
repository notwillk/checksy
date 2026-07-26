#!/usr/bin/env python3
"""Run KVM-only RulesyOS boot checks with Buildroot's runtime-test emulator."""

import argparse
import os
from pathlib import Path
import signal
import stat
import sys


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


def boot_known_good(args: argparse.Namespace) -> None:
    Emulator = load_emulator(args.buildroot)
    log_base = args.buildroot.parent.parent / "runtime" / "known-good"
    log_base.parent.mkdir(parents=True, exist_ok=True)
    emulator = Emulator(
        str(log_base),
        str(args.disk.parent),
        True,
        1,
    )

    def stop_on_signal(signum, _frame):
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGINT, stop_on_signal)
    signal.signal(signal.SIGTERM, stop_on_signal)

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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--buildroot", type=Path, required=True)
    subparsers = parser.add_subparsers(dest="test", required=True)
    known_good = subparsers.add_parser("known-good")
    known_good.add_argument("--disk", type=Path, required=True)
    known_good.add_argument("--kernel", type=Path, required=True)
    known_good.add_argument("--initramfs", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    if not kvm_is_ready():
        print(
            "WARNING: KVM acceleration is unavailable; skipping RulesyOS VM test.",
            file=sys.stderr,
        )
        return 0

    args = parse_args()
    if args.test == "known-good":
        boot_known_good(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
