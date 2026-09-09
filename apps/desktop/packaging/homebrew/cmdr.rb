cask "cmdr" do
  version "0.24.0"
  sha256 "2838f14ac160252178e16fae984e00b21d0a20de092d0c6738b2225bab899837"

  url "https://license.getcmdr.com/download/#{version}/universal"
  name "Cmdr"
  desc "Keyboard-driven dual-pane file manager"
  homepage "https://getcmdr.com/"

  livecheck do
    url "https://getcmdr.com/latest.json"
    strategy :json do |json|
      json["version"]
    end
  end

  auto_updates true
  depends_on macos: :big_sur

  app "Cmdr.app"

  # Hand "Show in Finder" back before leaving. macOS routes it by bundle id, through the
  # undocumented NSFileViewer key of the global preferences domain, and when that id names an
  # app that is gone it does NOT fall back to Finder: the command silently does nothing, in
  # every app, machine-wide, until someone clears the key by hand. Cmdr only ever writes it
  # when the user asks (Settings > Behavior > Navigation & file ops), so uninstalling has to
  # take it back out.
  #
  # In uninstall, never only in zap: zap runs on `brew uninstall --zap` alone, and a plain
  # `brew uninstall cmdr` is the case that would break someone's Mac. The test on the current
  # value is the other half: Path Finder and ForkLift write the same key, and clearing it
  # blind would unregister theirs. Only this one key is touched, never the plist holding it.
  #
  # Known cost: Homebrew runs uninstall directives for `brew reinstall` and for a `--greedy`
  # upgrade too, and nothing in the cask DSL distinguishes those from a real uninstall. Both
  # leave the switch off, which the user can turn back on in Settings. A dangling key can't be
  # turned back on by anyone.
  uninstall script: {
    executable:   "/bin/sh",
    args:         [
      "-c",
      'if [ "$(/usr/bin/defaults read -g NSFileViewer 2>/dev/null)" = "com.veszelovszki.cmdr" ]; then ' \
      "/usr/bin/defaults delete -g NSFileViewer; fi",
    ],
    must_succeed: false,
  }

  zap trash: [
    "~/Library/Application Support/com.veszelovszki.cmdr",
    "~/Library/Caches/com.veszelovszki.cmdr",
    "~/Library/Logs/com.veszelovszki.cmdr",
    "~/Library/Preferences/com.veszelovszki.cmdr.plist",
    "~/Library/Saved Application State/com.veszelovszki.cmdr.savedState",
    "~/Library/WebKit/com.veszelovszki.cmdr",
  ]
end
