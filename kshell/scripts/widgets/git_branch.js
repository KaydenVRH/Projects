// Example kshell JS widget: shows the current git branch of ~/dotfiles.
//
// Host API available: exec(cmd, interval, cb), execOnce(cmd, cb), shell(cmd),
// post(event, payload), battery(), ram(), volume(), date(pattern), log(msg).
// Define render() -> { icon, label, iconColor, labelColor, action,
//                      actionEvent, padding, flexible } and optionally
// onEvent(name, payload).

var branch = "";

exec("cd ~/dotfiles 2>/dev/null && git rev-parse --abbrev-ref HEAD 2>/dev/null", 5, function (out) {
  branch = out.trim();
});

function render() {
  return {
    icon: "\uf126",            // fa-code-fork
    label: branch || "—",
    iconColor: "#ff2a85",
    labelColor: "#00d4ff",
    actionEvent: "app_launcher_toggle"
  };
}
