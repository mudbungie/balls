.PHONY: build test check doc leak-scan \
	install install-core install-tracker install-delivery install-chore \
	install-speculate uninstall clean hooks \
	deploy-local deploy-status deploy-selftest

PREFIX ?= $(HOME)/.local
BIN_DIR := $(PREFIX)/bin

build:
	cargo build --release

test:
	cargo test

check: test doc
	cargo clippy --all-targets -- -D warnings
	scripts/check-line-lengths.sh
	scripts/deploy/update-selftest.sh
	scripts/check-coverage.sh

# The disclosure scan (bl-816b, from the rust-bootstrap template):
# scripts/leak-rules.sh is the table, leak-scan.sh the mechanism. --self-test
# first, because a leak gate dies by silently matching nothing. Not yet a step
# of `check` or the pre-commit chain: main carries findings the scan must be
# clean of first — run this target to see them. The machine-global balls plugin
# bl-leak-gate runs this same scanner over the TASK STORE before every publish,
# and .github/workflows/store-scan.yml re-judges the published balls/tasks ref.
leak-scan:
	@scripts/leak-scan.sh --self-test
	@scripts/leak-scan.sh

# THE blessed doc build (bl-3d09) — the only invocation this repo's docs are
# guaranteed warning-clean under, so it is the one to run and the one to trust.
#
#   --document-private-items  the docs are a connected map: a module note links
#                             to the private helper carrying the reasoning, so
#                             the private items must be in the rendered graph.
#   -D warnings               every rustdoc warning is an error. A broken link is
#                             a build failure, not a line lost in the scroll —
#                             which is the whole point (68 private-link warnings
#                             used to bury the real ones).
doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items

# The whole suite: core + every shipped sibling plugin. A core-only install
# leaves `bl` resolving no `bl-tracker`/`bl-delivery` beside it (Edge::resolve
# looks beside the bl binary), so `bl prime` founds stealth-only and federation
# silently never engages. Installing the plugins next to bl is what wires them.
install: install-core install-tracker install-delivery install-chore install-speculate

# Seat one built binary in BIN_DIR WITHOUT ever truncating a running one.
# `install`(1) opens the destination `O_TRUNC`, which is ETXTBSY against a
# binary a fleet of agents is executing right now — and a `bl` that fails to
# install mid-fleet is every agent's claim and close, not just this one's.
# Write a temp name in the SAME directory and rename(2) it into place: an
# executing process keeps its own inode, and every later exec gets whole-new.
# The same move `scripts/deploy/seat.sh` and `cargo install` both make.
define seat_bin
	install -d $(BIN_DIR)
	install -m 0755 target/release/$(1) $(BIN_DIR)/.$(1).tmp
	mv -f $(BIN_DIR)/.$(1).tmp $(BIN_DIR)/$(1)
endef

# Just the bl core binary + the `balls` alias.
install-core: build
	$(call seat_bin,bl)
	ln -sf bl $(BIN_DIR)/balls

# Each plugin is a sibling binary dropped beside bl in BIN_DIR; that adjacency
# is the whole wiring (§6 subprocess-uniform, resolved by Edge::resolve).
install-tracker: build
	$(call seat_bin,bl-tracker)

install-delivery: build
	$(call seat_bin,bl-delivery)

# §10 guarded-mint primitive (opt-in). Shipped beside bl like any plugin; wire
# it per checkout with `bl conf prepend claim.post bl-chore`.
install-chore: build
	$(call seat_bin,bl-chore)

# Verdict-cache edge (bl-1263, design bl-24e7). Consulted by scripts/pre-commit
# via PATH; fail-open, so installing it only ever removes redundant gate runs.
install-speculate: build
	$(call seat_bin,bl-speculate)

uninstall:
	rm -f $(BIN_DIR)/bl $(BIN_DIR)/balls $(BIN_DIR)/bl-tracker $(BIN_DIR)/bl-delivery \
		$(BIN_DIR)/bl-chore $(BIN_DIR)/bl-speculate

# Continuous deployment for THIS box (bl-4316): seat the systemd user timer
# that reconciles `bl` and its plugin set against crates.io hourly, then run
# the first reconcile synchronously so seating ends with an answer rather than
# a promise. Local-only and deliberately so — `bl` is reached at a prompt, and
# the box somebody is sitting at cannot ssh to itself. Reasoning lives in
# `scripts/deploy/seat.sh` and README "Keeping this box current".
deploy-local:
	scripts/deploy/seat.sh

# A read: what is installed, what the registry offers, whether the timer is
# armed. Installs nothing.
deploy-status:
	scripts/deploy/status.sh

# The real reconciler under fake curl/cargo shims, both directions. A step of
# `check`, because a reconciler that installs on every tick is as broken as one
# that never does and neither shows up in an exit code.
deploy-selftest:
	scripts/deploy/update-selftest.sh

# Install the repo-local pre-commit hook (line-length + clippy + tests
# + 100% coverage). Run once per clone; not part of `make install`
# because a user installing the binary should not have hooks wired
# into whatever repo they happen to be in.
hooks:
	scripts/install-hooks.sh

clean:
	cargo clean
