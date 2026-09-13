import os
from pathlib import Path
import subprocess
import tempfile
import unittest


PUBLISHER = Path(__file__).resolve().parents[1] / "scripts/publish-channel"


class ReleaseChannels(unittest.TestCase):
    def test_channels_follow_published_versions_without_downgrading(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            remote = root / "remote.git"
            work = root / "work"
            work.mkdir()
            env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")

            def git(*args):
                return subprocess.check_output(
                    ["git", *args], cwd=work, env=env, text=True, stderr=subprocess.DEVNULL
                ).strip()

            git("init", "--bare", str(remote))
            git("init")
            git("config", "user.name", "Release tests")
            git("config", "user.email", "release-tests@example.invalid")
            git("remote", "add", "origin", str(remote))

            def release(version):
                (work / "Cargo.toml").write_text(
                    f'[package]\nname = "jobsdone"\nversion = "{version}"\n'
                )
                git("add", "Cargo.toml")
                git("commit", "-m", version)
                git("tag", f"v{version}")
                return git("rev-parse", "HEAD")

            def publish(channel, version, success=True):
                result = subprocess.run(
                    [str(PUBLISHER), channel, f"v{version}"],
                    cwd=work, env=env, capture_output=True, text=True,
                )
                self.assertEqual(result.returncode == 0, success, result.stderr)

            def pointer(channel):
                return git("ls-remote", "origin", f"refs/heads/release-{channel}").split()[0]

            beta2 = release("0.1.0-beta.2")
            beta10 = release("0.1.0-beta.10")
            stable = release("0.1.0")
            next_beta = release("0.2.0-beta.1")
            publish("beta", "0.1.0-beta.2")
            self.assertEqual(pointer("beta"), beta2)
            publish("beta", "0.1.0-beta.10")
            publish("beta", "0.1.0-beta.2")
            self.assertEqual(pointer("beta"), beta10)
            publish("stable", "0.1.0-beta.10", success=False)
            publish("stable", "0.1.0")
            publish("beta", "0.1.0")
            self.assertEqual(pointer("stable"), stable)
            self.assertEqual(pointer("beta"), stable)
            publish("beta", "0.2.0-beta.1")
            publish("beta", "0.1.0")
            self.assertEqual(pointer("beta"), next_beta)
            self.assertEqual(pointer("stable"), stable)


if __name__ == "__main__":
    unittest.main()
