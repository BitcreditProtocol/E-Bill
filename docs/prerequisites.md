# Prerequisites

Make sure to have at least Rust version 1.85 as well as a recent version of the toolchain installed.

#### On Ubuntu
```bash
# Install libs
sudo apt install -y libclang-dev pkg-config build-essential
```

#### On Fedora
```bash
# Install libs
sudo dnf install -y make automake gcc gcc-c++ kernel-devel clang-devel
sudo dnf install -y pkgconf-pkg-config @development-tools
```

#### On Windows
```bash
# Using MSYS2 terminal
pacman -S mingw-w64-x86_64-gcc mingw-w64-x86_64-make mingw-w64-x86_64-llvm
pacman -S base-devel pkgconf
```
