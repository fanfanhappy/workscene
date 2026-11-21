/// 显示器工作区/窗口矩形的纯数学换算。
/// 全部为纯函数，便于单元测试（文档 §15.1 要求覆盖坐标转换与边界约束）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Area {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Area { x, y, w, h }
    }
}

/// 最小可恢复尺寸：避免把窗口压成无法再次拖动的细条
pub const MIN_W: f64 = 320.0;
pub const MIN_H: f64 = 200.0;

/// 窗口矩形 → 相对工作区的比例坐标
pub fn to_ratios(rect: Area, work: Area) -> (f64, f64, f64, f64) {
    let ww = if work.w <= 0.0 { 1.0 } else { work.w };
    let wh = if work.h <= 0.0 { 1.0 } else { work.h };
    (
        (rect.x - work.x) / ww,
        (rect.y - work.y) / wh,
        rect.w / ww,
        rect.h / wh,
    )
}

/// 比例坐标 → 当前工作区中的像素矩形（不做边界约束，交给 clamp）
pub fn from_ratios(r: (f64, f64, f64, f64), work: Area) -> Area {
    Area {
        x: work.x + r.0 * work.w,
        y: work.y + r.1 * work.h,
        w: r.2 * work.w,
        h: r.3 * work.h,
    }
}

/// 约束到可见工作区，并保证不小于最小尺寸。
/// 窗口比工作区还大时按工作区尺寸处理。
pub fn clamp(rect: Area, work: Area) -> Area {
    let w = rect.w.max(MIN_W).min(work.w.max(MIN_W));
    let h = rect.h.max(MIN_H).min(work.h.max(MIN_H));

    let max_x = work.x + work.w - w;
    let max_y = work.y + work.h - h;

    Area {
        x: rect.x.clamp(work.x, max_x.max(work.x)),
        y: rect.y.clamp(work.y, max_y.max(work.y)),
        w,
        h,
    }
}

/// 比例坐标归一化到 [0,1] 且 x+w ≤ 1。
/// 最大化的窗口实测会略大于工作区（含不可见边框，例如 -0.3% / 100.6%），
/// 直接存盘会让 UI 显示出"负偏移"，而恢复时本来也会被 clamp，所以存盘即归一。
pub fn normalize_ratios(r: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let clamp01 = |v: f64| v.clamp(0.0, 1.0);
    let x = clamp01(r.0);
    let y = clamp01(r.1);
    let w = clamp01(r.2).min(1.0 - x).max(0.0);
    let h = clamp01(r.3).min(1.0 - y).max(0.0);
    (x, y, w, h)
}

/// 四舍五入到整数像素（SetWindowPos 只接受整数）
pub fn round(rect: Area) -> (i32, i32, i32, i32) {
    (
        rect.x.round() as i32,
        rect.y.round() as i32,
        rect.w.round() as i32,
        rect.h.round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Area = Area {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1040.0,
    };

    #[test]
    fn ratio_roundtrip_is_stable() {
        let rect = Area::new(100.0, 80.0, 960.0, 520.0);
        let r = to_ratios(rect, WORK);
        let back = from_ratios(r, WORK);
        assert!((back.x - rect.x).abs() < 0.001);
        assert!((back.w - rect.w).abs() < 0.001);
    }

    #[test]
    fn ratio_is_relative_to_work_area_origin() {
        // 左侧副屏 x 为负（常见双屏配置），比例必须仍以各自工作区为原点
        let work = Area::new(-1920.0, 0.0, 1920.0, 1040.0);
        let rect = Area::new(-1920.0, 0.0, 960.0, 1040.0);
        let r = to_ratios(rect, work);
        assert!((r.0 - 0.0).abs() < 1e-9);
        assert!((r.2 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn clamp_pulls_window_back_into_visible_area() {
        // 目标屏变小：窗口右侧越界，应被拉回
        let small = Area::new(0.0, 0.0, 1280.0, 720.0);
        let rect = from_ratios((0.5, 0.5, 0.5, 0.5), small);
        let fixed = clamp(rect, small);
        assert!(fixed.x + fixed.w <= small.x + small.w + 0.001);
        assert!(fixed.y + fixed.h <= small.y + small.h + 0.001);
    }

    #[test]
    fn clamp_enforces_minimum_size() {
        let clamped = clamp(Area::new(10.0, 10.0, 20.0, 20.0), WORK);
        assert_eq!(clamped.w, MIN_W);
        assert_eq!(clamped.h, MIN_H);
    }

    #[test]
    fn round_is_pixel_exact() {
        let (x, y, w, h) = round(Area::new(10.4, 10.6, 100.5, 200.49));
        assert_eq!((x, y, w, h), (10, 11, 101, 200));
    }

    #[test]
    fn normalize_ratios_clamps_maximized_overshoot() {
        // 最大化窗口实测：x=-0.003, w=1.006
        let (x, y, w, h) = normalize_ratios((-0.003, -0.005, 1.006, 1.011));
        assert_eq!((x, y), (0.0, 0.0));
        assert!(w <= 1.0 && h <= 1.0);
        assert!((w - 1.0).abs() < 1e-9 && (h - 1.0).abs() < 1e-9);
    }

    #[test]
    fn normalize_ratios_keeps_in_range_values() {
        let r = normalize_ratios((0.25, 0.1, 0.5, 0.4));
        assert_eq!(r, (0.25, 0.1, 0.5, 0.4));
    }
}
