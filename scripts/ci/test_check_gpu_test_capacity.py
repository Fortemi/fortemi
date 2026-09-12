import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("capacity", Path(__file__).with_name("check-gpu-test-capacity.py"))
capacity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capacity)


class CapacityTests(unittest.TestCase):
    memory = "MemTotal: 64000000 kB\nMemAvailable: 32000000 kB\n"

    def test_accepts_reserved_headroom(self):
        self.assertEqual(capacity.check_capacity("24564, 18000\n", self.memory)["status"], "PASS")

    def test_refuses_current_contended_gpu(self):
        with self.assertRaisesRegex(ValueError, "capacity unavailable"):
            capacity.check_capacity("24564, 5600\n", self.memory)

    def test_refuses_low_or_missing_ram(self):
        for value in ["", "MemAvailable: 1024 kB\n", "MemAvailable: 32000000 bytes\n"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                capacity.check_capacity("24564, 18000", value)

    def test_refuses_ambiguous_or_invalid_gpu_observations(self):
        for value in ["", "24564, 18000\n24564, 18000", "24564", "24564, N/A", "24564, 30000", "24564, -1"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                capacity.check_capacity(value, self.memory)


if __name__ == "__main__":
    unittest.main()
