// 发布版不弹控制台窗口；--selftest / --m1test / --capture 走 debug 构建，仍有输出
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
    if let Some(pos) = args.iter().position(|a| a == "--capture") {
        let name = args.get(pos + 1).cloned().unwrap_or_else(|| "未命名现场".into());
        workscene_lib::cli_capture(&name);
        return;
    }

    workscene_lib::run()
}
