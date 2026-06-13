# Homebrew formula for cosmic-paste (builds from source).
# COSMIC desktop + Wayland session required at runtime.
class CosmicPaste < Formula
  desc "Clipboard manager for the COSMIC desktop"
  homepage "https://github.com/erikbalfe/cosmic-paste"
  version "0.1.0"
  license "BSD-2-Clause"

  head do
    url "https://github.com/erikbalfe/cosmic-paste.git", branch: "main"
  end

  depends_on "pkgconf" => :build
  depends_on "rust" => :build
  depends_on "libxkbcommon"
  depends_on "wayland"

  def install
    ENV["PKG_CONFIG_PATH"] = "#{HOMEBREW_PREFIX}/lib/pkgconfig"
    system "cargo", "install", *std_cargo_args(path: "cosmic-paste-cli")
    system "cargo", "install", "--root", prefix, "--path", "cosmic-paste-daemon"
    system "cargo", "install", "--root", prefix, "--path", "cosmic-paste-applet"
    bin.install "scripts/cosmic-paste-show-history"
    pkgshare.install "data/examples/cosmic-custom-shortcuts.ron"
  end

  def caveats
    <<~EOS
      cosmic-paste targets the COSMIC desktop on Linux (Wayland).

      After install on COSMIC:
        1. Copy the systemd user unit from the repo or run scripts/install.sh on Linux.
        2. Add the panel applet: Settings → Panel → Applets → COSMIC Paste.

      Keyboard shortcut examples: #{pkgshare}/cosmic-custom-shortcuts.ron
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cosmic-paste version")
  end
end