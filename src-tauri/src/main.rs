// 发布版不弹控制台窗口；诊断入口走 debug 构建，仍有输出
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Chrome/Edge 通过 Native Messaging 启动同一个 exe；此时不能进入 GUI。
    if workscene_lib::native_host::is_native_messaging_host() {
        workscene_lib::native_host::run();
        return;
    }

    // CLI 自检/基线也要留下日志，方便用户把失败现场发回来。
    workscene_lib::logging::init();
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--native-host") {
        workscene_lib::native_host::run();
        return;
    }
    if args.iter().any(|a| a == "--install-browser-host") {
        match workscene_lib::browser_bridge::install_native_host() {
            Ok(path) => eprintln!("native host manifest: {}", path.display()),
            Err(e) => eprintln!("install failed: {e}"),
        }
        return;
    }
    if args.iter().any(|a| a == "--selftest") {
        workscene_lib::selftest();
        return;
    }
    if args.iter().any(|a| a == "--m1test") {
        workscene_lib::m1test();
        return;
    }
    if args.iter().any(|a| a == "--m2test") {
        workscene_lib::m2test();
        return;
    }
    if args.iter().any(|a| a == "--m5test") {
        workscene_lib::m5test();
        return;
    }
    if args.iter().any(|a| a == "--m9test") {
        workscene_lib::m9test();
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--baseline-live") {
        let n = args
            .get(pos + 1)
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(6);
        workscene_lib::baseline_live(n);
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--baseline") {
        let rounds = args
            .get(pos + 1)
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(5);
        workscene_lib::baseline(rounds);
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--capture") {
        let name = args.get(pos + 1).cloned().unwrap_or_else(|| "未命名现场".into());
        workscene_lib::cli_capture(&name);
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--snap") {
        let name = args.get(pos + 1).cloned().unwrap_or_default();
        workscene_lib::cli_snapshot(&name);
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--autostart") {
        let mode = args.get(pos + 1).cloned().unwrap_or_else(|| "status".into());
        workscene_lib::cli_autostart(&mode);
        return;
    }

    workscene_lib::run()
}
