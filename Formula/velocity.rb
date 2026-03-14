class Velocity < Formula
  desc "Fast, reliable mobile UI testing framework"
  homepage "https://github.com/omkar-dev/velocity"
  license "Apache-2.0"
  head "https://github.com/omkar-dev/velocity.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "velocity-cli")
  end

  test do
    assert_match "velocity", shell_output("#{bin}/velocity version")
  end
end
