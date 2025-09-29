# setting up the self hosted runner

1- install Rust
2- add `/etc/sudoers.d/github-runner`
```
runner_username ALL=(ALL) NOPASSWD: /home/runner/.cargo/bin/cargo test run_as_root *
```
3- install packages
```
sudo apt-get install -y build-essential pkg-config libssl-dev libdevmapper-dev lvm2 clang libclang-dev
```
