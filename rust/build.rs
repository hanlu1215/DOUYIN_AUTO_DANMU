// 构建脚本：把 app.rc（图标 + 应用清单）编译进 exe。
//
// 清单内容：Comctl32 v6（控件新版视觉样式）+ PerMonitorV2 DPI 声明。
// 找不到资源编译器（rc.exe / windres / llvm-rc）时跳过 —— 程序功能不受影响，
// 仅控件外观退回经典样式；DPI 感知仍由 main.rs 的运行时 API 兜底。
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ok = std::panic::catch_unwind(|| {
        embed_resource::compile("app.rc", embed_resource::NONE);
    });
    if ok.is_err() {
        println!("cargo:warning=未找到资源编译器，图标与清单未嵌入；程序仍可正常运行");
    }
}
