# typed: false
# frozen_string_literal: true

# This formula is auto-updated by the release workflow.
# Manual edits will be overwritten on next release.
class Camgrab < Formula
  desc "A modern CLI tool for capturing snapshots, recording clips, and managing RTSP/ONVIF cameras"
  homepage "https://github.com/justinhuangcode/camgrab"
  license "MIT"
  version "1.0.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/justinhuangcode/camgrab/releases/download/v#{version}/camgrab-v#{version}-macos-arm64.tar.gz"
      sha256 "PLACEHOLDER"
    else
      url "https://github.com/justinhuangcode/camgrab/releases/download/v#{version}/camgrab-v#{version}-macos-x86_64.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/justinhuangcode/camgrab/releases/download/v#{version}/camgrab-v#{version}-linux-arm64.tar.gz"
      sha256 "PLACEHOLDER"
    else
      url "https://github.com/justinhuangcode/camgrab/releases/download/v#{version}/camgrab-v#{version}-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "camgrab"
  end

  test do
    assert_match "camgrab", shell_output("#{bin}/camgrab --version")
  end
end
