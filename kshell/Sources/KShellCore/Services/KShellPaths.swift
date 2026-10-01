import Foundation

/// Canonical locations for kshell config and user scripts.
public enum KShellPaths {
    public static var configDirectory: URL {
        let environment = ProcessInfo.processInfo.environment
        let base: URL
        if let xdg = environment["XDG_CONFIG_HOME"], !xdg.isEmpty {
            base = URL(fileURLWithPath: xdg, isDirectory: true)
        } else {
            base = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".config", isDirectory: true)
        }
        return base.appendingPathComponent("kshell", isDirectory: true)
    }

    /// Where kshell keeps caches (artwork, metadata, media sidecars).
    public static var cacheDirectory: URL {
        let base = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Caches")
        return base.appendingPathComponent("kshell", isDirectory: true)
    }

    public static var configFile: URL {
        configDirectory.appendingPathComponent("config.toml")
    }

    /// Resolve `~`, absolute, or config-relative paths.
    public static func resolve(_ path: String) -> URL {
        let expanded = (path as NSString).expandingTildeInPath
        if expanded.hasPrefix("/") {
            return URL(fileURLWithPath: expanded)
        }
        return configDirectory.appendingPathComponent(expanded)
    }
}
