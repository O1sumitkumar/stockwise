#: Template for Formula/stockwise.rb in iamsaquib8/homebrew-tap. packaging/render.sh
#: fills the @PLACEHOLDERS@ from a release's .sha256 files and drops these #: lines;
#: the `homebrew` job in .github/workflows/release.yml pushes the result on each tag.
class Stockwise < Formula
  desc "Stock market toolkit for your terminal: Indian (NSE/BSE) and US stocks"
  homepage "https://github.com/iamsaquib8/stockwise"
  version "@VERSION@"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/iamsaquib8/stockwise/releases/download/v@VERSION@/stockwise-aarch64-apple-darwin.tar.gz"
      sha256 "@SHA256_AARCH64_APPLE_DARWIN@"
    end
    on_intel do
      url "https://github.com/iamsaquib8/stockwise/releases/download/v@VERSION@/stockwise-x86_64-apple-darwin.tar.gz"
      sha256 "@SHA256_X86_64_APPLE_DARWIN@"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/iamsaquib8/stockwise/releases/download/v@VERSION@/stockwise-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "@SHA256_AARCH64_UNKNOWN_LINUX_GNU@"
    end
    on_intel do
      url "https://github.com/iamsaquib8/stockwise/releases/download/v@VERSION@/stockwise-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "@SHA256_X86_64_UNKNOWN_LINUX_GNU@"
    end
  end

  def install
    bin.install "stockwise"
    man1.install Dir["man/*.1"]
    generate_completions_from_executable(bin/"stockwise", "completions")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/stockwise --version")
    assert_path_exists man1/"stockwise-quote.1"
    assert_match "SUBCOMMANDS", shell_output("#{bin}/stockwise man")
  end
end
