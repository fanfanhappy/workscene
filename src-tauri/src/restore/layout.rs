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

/// 最小可编辑尺寸（比例），与 UI 拖拽下限一致
pub const MIN_RATIO: f64 = 0.08;

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

/// 比例坐标归一化。
///
/// 允许轻微的负偏移与超出 1：真实窗口带不可见边框，最大化/贴边窗口实测会落在
/// (-0.0086, 1.006) 这种位置。早期版本把它们夹成 0 会导致恢复时整体偏移
/// （实测资源管理器被推了 33px）。这里只做"别离谱"的约束，是否可见交给
/// `clamp_visible`。
pub fn normalize_ratios(r: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let finite = |v: f64, fallback: f64| if v.is_finite() { v } else { fallback };
    let x = finite(r.0, 0.0).clamp(-0.25, 1.0);
    let y = finite(r.1, 0.0).clamp(-0.25, 1.0);
    // 宽高缺失/非法时退化为占满工作区，而不是缩成一个点
    let w = finite(r.2, 1.0).clamp(MIN_RATIO, 1.0);
    let h = finite(r.3, 1.0).clamp(MIN_RATIO, 1.0);
    (x, y, w, h)
}

/// 至少要有 `visible` 比例的面积落在工作区内，不让窗口跑到看不见的地方。
/// 与 `clamp` 的区别：不强制完全塞进工作区（否则贴边/带边框的窗口会被推移）。
pub fn clamp_visible(rect: Area, work: Area) -> Area {
    const VISIBLE: f64 = 0.3;
    let min_w = (rect.w * VISIBLE).max(80.0);
    let min_h = (rect.h * VISIBLE).max(60.0);

    // 工作区极小时 lo 可能大于 hi，f64::clamp 会 panic；先保证区间有序。
    let lo_x = work.x - rect.w + min_w;
    let hi_x = (work.x + work.w - min_w).max(lo_x);
    let lo_y = work.y - rect.h + min_h;
    let hi_y = (work.y + work.h - min_h).max(lo_y);
    let x = rect.x.clamp(lo_x, hi_x);
    let y = rect.y.clamp(lo_y, hi_y);

    Area { x, y, ..rect }
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
    fn normalize_ratios_keeps_small_negative_offsets() {
        // 真实窗口贴边时 x 可能略小于 0，不能夹掉（否则恢复会整体偏移）
        let (x, y, w, h) = normalize_ratios((-0.0086, -0.005, 1.006, 1.011));
        assert!((x + 0.0086).abs() < 1e-9, "轻微负偏移应保留");
        assert!((y + 0.005).abs() < 1e-9);
        assert!(w <= 1.0 && h <= 1.0);
    }

    #[test]
    fn normalize_ratios_rejects_absurd_values() {
        let (x, y, w, h) = normalize_ratios((f64::NAN, 9.0, -1.0, f64::INFINITY));
        assert_eq!(x, 0.0);
        assert_eq!(y, 1.0);
        assert!(w >= MIN_RATIO);
        assert!((h - 1.0).abs() < 1e-9);
    }

    #[test]
    fn clamp_visible_allows_partial_offscreen_but_not_lost() {
        // 贴左边、部分在屏幕外：允许保留
        let rect = Area::new(-33.0, 220.0, 1709.0, 962.0);
        let kept = clamp_visible(rect, WORK);
        assert_eq!(kept.x, -33.0, "已经足够可见，不应被推动");

        // 完全跑到屏幕外：拉回到至少可见 30%
        let lost = Area::new(-2000.0, 220.0, 1709.0, 962.0);
        let fixed = clamp_visible(lost, WORK);
        assert!(fixed.x + fixed.w > WORK.x + 80.0, "必须回到可见区域");
    }

    #[test]
    fn clamp_visible_does_not_panic_on_tiny_work_area() {
        // 极小工作区会让可见比例下界超过上界；必须收敛为有效区间而不是 panic。
        let tiny = Area::new(0.0, 0.0, 100.0, 80.0);
        let rect = Area::new(0.0, 0.0, 100.0, 80.0);
        let fixed = clamp_visible(rect, tiny);
        assert!(fixed.x.is_finite() && fixed.y.is_finite());
    }

    #[test]
    fn normalize_ratios_keeps_in_range_values() {
        let r = normalize_ratios((0.25, 0.1, 0.5, 0.4));
        assert_eq!(r, (0.25, 0.1, 0.5, 0.4));
    }
}
