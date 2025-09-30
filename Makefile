.PHONY: test
test:
	@env -u CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER cargo nextest run
	@env -u CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER cargo test --doc
	@echo "running root tests"
	@CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER="sudo -E" cargo nextest run run_as_root -- --ignored
