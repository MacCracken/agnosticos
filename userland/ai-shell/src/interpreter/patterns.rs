use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// Compiled regex patterns, shared across all Interpreter instances.
pub(crate) static PATTERNS: Lazy<HashMap<String, Regex>> = Lazy::new(|| {
    let mut p = HashMap::new();
    let mut r = |name: &str, pat: &str| {
        p.insert(name.to_string(), Regex::new(pat).unwrap());
    };
    r(
        "list",
        r"(?i)^(show|list|display|what|see)?\s*(me\s+)?(all\s+)?(files|directories|dirs|folders|contents?)?\s*(in\s+)?(.+)?$",
    );
    r(
        "show_file",
        r"(?i)^(show|display|view|read|cat|open|print)\s+(me\s+)?(the\s+)?(content|file|contents)?\s*(of\s+)?(.+)$",
    );
    r(
        "find",
        r"(?i)^(find|locate|search\s+for|look\s+for)\s+(files?\s+(named|called)?\s+)?(.+)(\s+in\s+(.+))?$",
    );
    r(
        "grep",
        r"(?i)^(search|grep|find)\s+(for\s+)?(.+?)\s+(in|within|inside)\s+(.+)$",
    );
    r(
        "cd",
        r"(?i)^(go\s+to|change\s+(to\s+)?|cd\s+(to\s+)?|switch\s+to)\s*(directory\s+)?(.+)$",
    );
    r(
        "mkdir",
        r"(?i)^(create|make|new)\s+(a\s+)?(new\s+)?(directory|folder)\s+(named|called)?\s*(.+)$",
    );
    r("copy", r"(?i)^(copy|duplicate)\s+(.+?)\s+(to|into)\s+(.+)$");
    r("move", r"(?i)^(move|rename)\s+(.+?)\s+(to|into|as)\s+(.+)$");
    r(
        "remove",
        r"(?i)^(remove|delete|rm)\s+(the\s+)?(file|directory|folder)?\s*(.+)$",
    );
    r(
        "ps",
        r"(?i)^(show|list|display|what|view)\s+(me\s+)?(all\s+)?(running\s+)?(processes|tasks|programs|apps)$",
    );
    r(
        "sysinfo",
        r"(?i)^(show|display|what|get|view)\s+(me\s+)?(system|computer|machine)\s*(info|information|status|stats)?$",
    );
    r(
        "du",
        r"(?i)^(how\s+much\s+)?(disk\s+)?(space|usage|size)\s+(is\s+)?(used\s+)?(by\s+)?(in\s+)?(.+)?$",
    );
    r(
        "install",
        r"(?i)^(install|add|get)\s+(package|program|software|app)?\s*(.+)$",
    );
    r(
        "audit",
        r"(?i)^(show|view|display|check)\s+(the\s+)?(audit|security)\s*(log|trail|history|entries)?(\s+for\s+(agent\s+)?(.+?))?(\s+(in|from)\s+(the\s+)?(last\s+)?(.+))?$",
    );
    r(
        "agent_info",
        r"(?i)^(show|list|view|display|what)\s+(me\s+)?(all\s+)?(running\s+)?(agents?|ai\s+agents?)\s*(status|info)?(\s+(.+))?$",
    );
    r(
        "service",
        r"(?i)^(list|show|start|stop|restart|status)\s+(the\s+)?(services?|daemons?)\s*(.+)?$",
    );
    r(
        "network_scan",
        r"(?i)^(scan\s+ports?\s+(?:on|for)\s+(.+)|ping\s+sweep\s+(.+)|lookup\s+dns\s+(?:for\s+)?(.+)|trace\s+route\s+to\s+(.+)|capture\s+packets?\s+(?:on|from)\s+(.+)|scan\s+web\s+servers?\s+(.+))$",
    );
    r(
        "network_extended",
        r"(?i)^(mass\s+scan\s+(.+)|arp\s+scan\s*(.+)?|network\s+diag(?:nostics?)?\s+(?:for\s+)?(.+)|detect\s+services?\s+(?:on\s+)?(.+)|fuzz\s+dir(?:ectories|s)?\s+(?:on\s+)?(.+)|vuln(?:erability)?\s+scan\s+(.+)|show\s+(?:open\s+)?sockets?|list\s+(?:network\s+)?connections?|enumerate\s+dns\s+(?:for\s+)?(.+)|deep\s+inspect\s+(?:traffic\s+)?(?:on\s+)?(.+)|monitor\s+bandwidth)$",
    );
    r(
        "journal",
        r"(?i)^(show|view|display|check)\s+(the\s+)?(journal|journald?|systemd)\s*(logs?|entries|messages)?(\s+for\s+(.+?))?(\s+since\s+(.+))?$",
    );
    r(
        "journal_alt",
        r"(?i)^(show|view|display)\s+(the\s+)?(last\s+(\d+)\s+)?(error|warning|critical|info|debug|notice|alert|emerg)?\s*(logs?|log\s+entries)(\s+for\s+(.+?))?(\s+since\s+(.+))?$",
    );
    r(
        "device_info",
        r"(?i)^(list|show|view|display)\s+(the\s+)?(all\s+)?(usb|block|net|pci|input|scsi)?\s*(devices?|hardware)(\s+(info|information|details))?(\s+for\s+(.+))?$",
    );
    r(
        "device_path",
        r"(?i)^(device|udev)\s+(info|information|details)\s+(for|on|about)\s+(.+)$",
    );
    r(
        "mount",
        r"(?i)^(list|show|display)\s+(the\s+)?(all\s+)?(fuse\s+)?(mounts?|mounted\s+filesystems?|filesystems?)$",
    );
    r(
        "unmount",
        r"(?i)^(unmount|umount|eject|fusermount\s+-u)\s+(.+)$",
    );
    r("mount_action", r"(?i)^mount\s+(.+?)\s+(on|at|to)\s+(.+)$");
    r(
        "boot",
        r"(?i)^(list|show|view|display)\s+(the\s+)?(boot\s+(entries|config|configuration|menu)|bootloader)$",
    );
    r(
        "boot_set",
        r"(?i)^set\s+(default\s+)?boot\s+(entry|default|timeout)\s+(to\s+)?(.+)$",
    );
    r(
        "update",
        r"(?i)^(check\s+for\s+updates?|apply\s+(system\s+)?updates?|rollback\s+(system\s+)?updates?|update\s+status|show\s+(current\s+)?version|system\s+update\s+(check|apply|rollback|status))$",
    );
    r(
        "question",
        r"(?i)^(what|who|when|where|why|how|is|are|can|do|does)\s+.+\??$",
    );
    r(
        "knowledge",
        r"(?i)^(search|find|look\s+up)\s+(in\s+)?(knowledge|kb|docs|documentation)\s+(for\s+)?(.+)$",
    );
    r(
        "rag_query",
        r"(?i)^(rag|retrieve|context)\s+(query|search|find|for)\s+(.+)$",
    );
    r("ark_install", r"(?i)^ark\s+install\s+(.+)$");
    r("ark_remove", r"(?i)^ark\s+(remove|uninstall)\s+(.+)$");
    r("ark_search", r"(?i)^ark\s+search\s+(.+)$");
    r("ark_info", r"(?i)^ark\s+(info|show)\s+(.+)$");
    r("ark_update", r"(?i)^ark\s+update$");
    r("ark_upgrade", r"(?i)^ark\s+upgrade(\s+(.+))?$");
    r("ark_status", r"(?i)^ark\s+status$");
    r(
        "marketplace_install",
        r"(?i)^(install|add)\s+(package|agent|app)\s+(.+)$",
    );
    r(
        "marketplace_uninstall",
        r"(?i)^(uninstall|remove)\s+(package|agent|app)\s+(.+)$",
    );
    r(
        "marketplace_search",
        r"(?i)^(search|find|browse)\s+(marketplace|market|store|packages|agents)\s+(for\s+)?(.+)$",
    );
    r(
        "marketplace_list",
        r"(?i)^(list|show)\s+(installed\s+)?(packages|marketplace|agents|apps)$",
    );
    r(
        "marketplace_update",
        r"(?i)^(update|upgrade)\s+(packages|agents|all)$",
    );
    r(
        "aequi_tax",
        r"(?i)^(show|what|get|estimate|calculate)\s+(my\s+)?(quarterly\s+)?tax\s*(estimate|liability)?(\s+(?:for\s+)?q?(\d))?$",
    );
    r(
        "aequi_schedule_c",
        r"(?i)^(show|get|preview|generate)\s+(my\s+)?schedule\s*c(\s+(?:for\s+)?(\d{4}))?$",
    );
    r(
        "aequi_import",
        r"(?i)^import\s+(bank\s+)?(statement|transactions?|ofx|qfx|csv)\s+(from\s+)?(.+)$",
    );
    r(
        "aequi_balance",
        r"(?i)^(show|get|what|view)\s+(my\s+)?(account\s+)?balance[s]?$",
    );
    r(
        "aequi_receipts",
        r"(?i)^(show|list|view)\s+(my\s+)?(pending\s+|reviewed\s+|unreviewed\s+)?receipts$",
    );
    r(
        "task_list",
        r"(?i)^(show|list|view)\s+(my\s+)?tasks(\s+(?:that are\s+|in\s+|with status\s+)(\w+))?$",
    );
    r(
        "task_create",
        r"(?i)^(create|add|new)\s+task[:\s]+(.+?)(\s+priority\s+(low|medium|high))?$",
    );
    r(
        "task_update",
        r"(?i)^(mark|update|set)\s+task\s+(\S+)\s+(?:as\s+|status\s+(?:to\s+)?)(\w+)$",
    );
    r(
        "ritual_check",
        r"(?i)^(?:show|check|how are)\s+(?:my\s+)?(?:rituals|habits)(\s+today|\s+(\d{4}-\d{2}-\d{2}))?$",
    );
    r(
        "productivity_stats",
        r"(?i)^(?:show\s+)?(?:my\s+)?(?:productivity|stats|statistics|analytics)(\s+(daily|weekly|monthly|this week|this month))?$",
    );
    // --- Agnostic QA platform intents ---
    r(
        "agnostic_run",
        r"(?i)^(?:agnostic\s+)?run\s+(?:test\s+)?suite\s+(.+?)(\s+(?:on|against|for)\s+(.+))?$",
    );
    r(
        "agnostic_status",
        r"(?i)^(?:agnostic\s+)?test\s+(?:run\s+)?status\s+(?:for\s+)?(\S+)$",
    );
    r(
        "agnostic_report",
        r"(?i)^(?:agnostic\s+)?test\s+report\s+(?:for\s+)?(\S+)(\s+(?:as\s+|format\s+)(\w+))?$",
    );
    r(
        "agnostic_list_suites",
        r"(?i)^(?:agnostic\s+)?(?:list|show)\s+(?:test\s+)?suites?(\s+(?:in\s+|category\s+)?(\w+))?$",
    );
    r(
        "agnostic_agents",
        r"(?i)^(?:agnostic\s+)?(?:qa\s+)?agent\s+status(\s+(?:for\s+)?(\S+))?$",
    );
    // --- Delta code hosting intents ---
    r(
        "delta_create_repo",
        r"(?i)^delta\s+create[\s-]+(repo|repository)\s+(\S+)(\s+(.+))?$",
    );
    r(
        "delta_list_repos",
        r"(?i)^delta\s+(list|show)\s+(repos|repositories)$",
    );
    r(
        "delta_pr",
        r"(?i)^delta\s+(pr|pull[\s-]?request)\s+(list|create|merge|close)(\s+(?:in\s+|for\s+)?(\S+))?(\s+(.+))?$",
    );
    r(
        "delta_push",
        r"(?i)^delta\s+push(\s+(\S+))?(\s+(?:branch\s+)?(\S+))?$",
    );
    r(
        "delta_ci",
        r"(?i)^delta\s+(ci|pipeline|build)\s+(status|logs?)(\s+(?:for\s+|in\s+)?(\S+))?$",
    );
    // --- Edge fleet management intents ---
    r(
        "edge_list",
        r"(?i)^(?:edge\s+)?(list|show)\s+(?:edge\s+)?nodes?(\s+(?:status\s+)?(\S+))?$",
    );
    r(
        "edge_deploy",
        r"(?i)^(?:edge\s+)?deploy\s+(?:to\s+edge\s+)?(.+?)(?:\s+(?:on|to)\s+(?:node\s+)?(\S+))?$",
    );
    r(
        "edge_update",
        r"(?i)^(?:edge\s+update\s+(?:edge\s+)?(?:node\s+)?(\S+)|update\s+(?:edge\s+)?node\s+(\S+))(?:\s+(?:to\s+)?(?:version\s+)?(\S+))?$",
    );
    r(
        "edge_health",
        r"(?i)^(?:edge\s+)?(health|status)\s+(?:of\s+)?(?:(?:edge\s+)?(?:node\s+)?(\S+)|(?:fleet|all|nodes))$",
    );
    r(
        "edge_decommission",
        r"(?i)^(?:edge\s+)?decommission\s+(?:edge\s+)?(?:node\s+)?(\S+)$",
    );
    p
});
