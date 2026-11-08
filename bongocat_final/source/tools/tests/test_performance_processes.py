"""Windows sampler races must not terminate the application under measurement."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "tools/performance-processes.ps1"


@unittest.skipUnless(sys.platform == "win32", "Windows process sampler")
class PerformanceProcessTests(unittest.TestCase):
    def test_sha256_does_not_require_the_filehash_cmdlet_and_releases_the_file(self):
        result = self.run_script("""
function Get-FileHash { throw 'FileHash cmdlet unavailable' }
$hashPath=Join-Path $PSScriptRoot 'hash.bin'
[IO.File]::WriteAllBytes($hashPath,[Text.Encoding]::UTF8.GetBytes('abc'))
$digest=Get-PerformanceFileHash -LiteralPath $hashPath
[IO.File]::Delete($hashPath)
@{sha256=$digest;deleted=(-not [IO.File]::Exists($hashPath))} | ConvertTo-Json -Compress
""")
        self.assertEqual(result, {"sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "deleted": True})

    def run_script(self, body):
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "sample.ps1"
            script.write_text("$ErrorActionPreference='Stop'\n. '" + str(HELPER).replace("'", "''") + "'\n" + body,
                              encoding="utf-8-sig")
            environment = os.environ.copy()
            environment.pop("PSModulePath", None)
            result = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(script)],
                                    cwd=ROOT, env=environment, capture_output=True, text=True, encoding="utf-8", errors="replace",
                                    creationflags=subprocess.CREATE_NO_WINDOW, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr)
            return json.loads(result.stdout)

    def mock_process(self, start):
        return """
$script:disposed=$false
$script:observed=[pscustomobject]@{HasExited=$false}
$script:observed | Add-Member -MemberType ScriptMethod -Name Dispose -Value {$script:disposed=$true}
$script:observed | Add-Member -MemberType ScriptProperty -Name StartTime -Value {START}
function Get-Process { param($Id,$ErrorAction) return $script:observed }
$sample=Get-PerformanceProcessSample -ProcessId 123 -ExpectedCreationTime ([datetime]'2026-01-01')
@{empty=($null -eq $sample);disposed=$script:disposed} | ConvertTo-Json -Compress
""".replace("START", start)

    def test_null_start_time_from_exiting_child_is_skipped_and_disposed(self):
        self.assertEqual(self.run_script(self.mock_process("$null")), {"empty": True, "disposed": True})

    def test_exception_from_exiting_child_is_skipped_and_disposed(self):
        self.assertEqual(self.run_script(self.mock_process("throw 'process exited'")), {"empty": True, "disposed": True})

    def test_reused_pid_is_not_attributed_to_the_previous_child(self):
        self.assertEqual(self.run_script(self.mock_process("[datetime]'2026-01-02'")), {"empty": True, "disposed": True})

    def test_live_process_produces_an_independent_resource_sample(self):
        result = self.run_script("""
$current=Get-Process -Id $PID
$started=$current.StartTime
$current.Dispose()
$sample=Get-PerformanceProcessSample -ProcessId $PID -ExpectedCreationTime $started
@{name=$sample.name;handles=$sample.handles;threads=$sample.threads;commit=$sample.private_commit} | ConvertTo-Json -Compress
""")
        self.assertEqual(result["name"], "powershell")
        self.assertGreater(result["handles"], 0)
        self.assertGreater(result["threads"], 0)
        self.assertGreater(result["commit"], 0)

    def test_exit_during_property_copy_does_not_publish_a_partial_sample(self):
        result = self.run_script("""
$script:disposed=$false
$script:checks=0
$script:observed=[pscustomobject]@{StartTime=[datetime]'2026-01-01';ProcessName='ephemeral';TotalProcessorTime=[TimeSpan]::Zero;PrivateMemorySize64=1;WorkingSet64=1;HandleCount=1;Threads=@(1)}
$script:observed | Add-Member -MemberType ScriptMethod -Name Dispose -Value {$script:disposed=$true}
$script:observed | Add-Member -MemberType ScriptProperty -Name HasExited -Value {$script:checks=$script:checks+1;return ($script:checks -gt 1)}
function Get-Process { param($Id,$ErrorAction) return $script:observed }
$sample=Get-PerformanceProcessSample -ProcessId 123 -ExpectedCreationTime ([datetime]'2026-01-01')
@{empty=($null -eq $sample);disposed=$script:disposed} | ConvertTo-Json -Compress
""")
        self.assertEqual(result, {"empty": True, "disposed": True})

    def test_parent_pid_reuse_does_not_adopt_preexisting_processes(self):
        result = self.run_script("""
$known=[Collections.Generic.HashSet[int]]::new();[void]$known.Add(42)
$created=@{};$created[42]=[datetime]'2026-01-02'
$old=[pscustomobject]@{ParentProcessId=42;CreationDate=[datetime]'2026-01-01'}
$new=[pscustomobject]@{ParentProcessId=42;CreationDate=[datetime]'2026-01-03'}
$untracked=[pscustomobject]@{ParentProcessId=43;CreationDate=[datetime]'2026-01-03'}
@{old=(Test-PerformanceDescendant $old $known $created);new=(Test-PerformanceDescendant $new $known $created);untracked=(Test-PerformanceDescendant $untracked $known $created)} | ConvertTo-Json -Compress
""")
        self.assertEqual(result, {"old": False, "new": True, "untracked": False})

    def test_window_guard_requires_matching_process_time_and_directory(self):
        native = str(ROOT / "tools/performance-windows.cs").replace("'", "''")
        result = self.run_script("Add-Type -Path '" + native + "'\n" + """
$current=Get-Process -Id $PID
$created=$current.StartTime.ToUniversalTime().ToFileTimeUtc()
$current.Dispose()
@{same=[ProfileWindows]::OwnsProcess($PID,$created,$PSHOME);other_directory=[ProfileWindows]::OwnsProcess($PID,$created,[IO.Path]::GetTempPath());other_time=[ProfileWindows]::OwnsProcess($PID,($created+10000000),$PSHOME)} | ConvertTo-Json -Compress
""")
        self.assertEqual(result, {"same": True, "other_directory": False, "other_time": False})


if __name__ == "__main__":
    unittest.main()
