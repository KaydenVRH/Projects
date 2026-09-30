// Example kshell JS widget: shows the current git branch of ~/dotfiles.
//
// Host API available: exec(cmd, interval, cb), execOnce(cmd, cb), shell(cmd),
// post(event, payload), battery(), ram(), volume(), date(pattern), log(msg).
// Define render() -> { icon, label, iconColor, labelColor, action,
//                      actionEvent, padding, flexible } and optionally
// onEvent(name, payload).
//
// Colors accept hex, or a theme token — "$accent", "$highlight", "$foreground",
// "$dim", "$mid", "$background" — so the widget follows the active theme instead
// of being stuck on one palette.

var branch = "";

exec("cd ~/dotfiles 2>/dev/null && git rev-parse --abbrev-ref HEAD 2>/dev/null", 5, function (out) {
  branch = out.trim();
});

function render() {
  return {
    icon: "\uf126",            // fa-code-fork
    label: branch || "—",
    iconColor: "$accent",
    labelColor: "$highlight",
    actionEvent: "app_launcher_toggle"
  };
}
