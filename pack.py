import platform
import tarfile
from os import mkdir

os = platform.system().lower()

mkdir("deploy")

binary = "lwoss.exe" if os == "windows" else "lwoss"

with tarfile.open(f'deploy/lwoss-{os}.tar.xz', 'w:xz') as tar:
    tar.add("web/build", "web")
    tar.add(f"target/release/{binary}", binary)
