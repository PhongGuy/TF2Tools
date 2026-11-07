# One command surface for contributors and CI: bare `make` installs TF2Tools, `make demo` opens it
# on a seeded throwaway library, `make check` is the CI gate and `make gate` the local one.
CARGO ?= cargo
# Build jobs for install and demo, low enough to leave room for other work on the machine.
JOBS ?= 4
# Install root (default ~/.local), taken only from `make PREFIX=...`, never from the environment.
PREFIX :=

.DEFAULT_GOAL := install
.PHONY: install uninstall demo check gate fmt fmt-check lint test pilot-check scripts-check package package-check windows-check readme-shots

install:
	CARGO="$(CARGO)" JOBS="$(JOBS)" TF2TOOLS_PREFIX="$(PREFIX)" scripts/install.sh install

uninstall:
	TF2TOOLS_PREFIX="$(PREFIX)" scripts/install.sh uninstall

demo:
	CARGO="$(CARGO)" CARGO_BUILD_JOBS="$(JOBS)" scripts/demo.sh

check: fmt-check lint test

# Linux release packages (deb and AppImage); kept out of check and gate since a release build takes minutes.
PACKAGES ?= $(or $(CARGO_TARGET_DIR),target)/packages
# Absolute however PACKAGES is set, since cargo-packager resolves a relative out dir inside the crate.
PACKAGES_DIR = $(abspath $(PACKAGES))
PACKAGER_VERSION := 0.11.8
# NO_STRIP: linuxdeploy's bundled strip cannot read newer .relr.dyn libraries and aborts the AppImage.
package:
	@[ "$$($(CARGO) packager --version 2> /dev/null)" = "cargo-packager $(PACKAGER_VERSION)" ] \
		|| { echo "make package needs cargo-packager $(PACKAGER_VERSION): cargo install cargo-packager --version $(PACKAGER_VERSION) --locked" >&2; exit 1; }
	CARGO_BUILD_JOBS="$(JOBS)" $(CARGO) build --release --locked -p tf2tools-app --bin tf2tools
	mkdir -p "$(PACKAGES_DIR)"
	rm -f "$(PACKAGES_DIR)"/*.deb "$(PACKAGES_DIR)"/*.AppImage
	NO_STRIP=1 $(CARGO) packager --release -p tf2tools-app -o "$(PACKAGES_DIR)"

package-check: package
	scripts/test-package.sh "$(PACKAGES_DIR)"

# The README screenshots, taken from the real app against your own library; kept out of check and gate since it drives the app.
readme-shots:
	CARGO="$(CARGO)" scripts/readme-screenshots.sh

# Clippy for the Windows target in a mingw container; kept out of check and gate since it needs podman.
windows-check:
	scripts/check-windows.sh lint

# The local merge gate: CI runs only `check`, since the pilot crate needs its git host dependency.
gate: check pilot-check scripts-check

# The install and demo scripts against temp dirs and a stub binary, the trailer check against a
# throwaway repository (never this one's history) and the release notes against canned GitHub notes.
scripts-check:
	scripts/test-scripts.sh
	scripts/test-check-trailers.sh
	scripts/test-release-notes.sh

fmt:
	$(CARGO) fmt

fmt-check:
	$(CARGO) fmt --check

lint:
	$(CARGO) clippy --all-targets --all-features -- -Dwarnings

# Tests must never open a window on the developer's display.
test:
	env -u WAYLAND_DISPLAY -u DISPLAY $(CARGO) test --workspace

# The pilot crate sits outside the workspace with its own git host dependency, so CI never builds it.
PILOT := --manifest-path tools/tf2tools-pilot/Cargo.toml
pilot-check:
	$(CARGO) fmt --check $(PILOT)
	$(CARGO) clippy $(PILOT) --all-targets --all-features -- -Dwarnings
	env -u WAYLAND_DISPLAY -u DISPLAY $(CARGO) test $(PILOT)
