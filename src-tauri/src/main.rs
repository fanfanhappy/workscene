// 发布版不弹控制台窗口；诊断入口走 debug 构建，仍有输出
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

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

    workscene_lib::run()
}
