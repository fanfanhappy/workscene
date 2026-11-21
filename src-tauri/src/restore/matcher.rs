use crate::core::models::SceneWindow;

/// 当前桌面上一个候选窗口（与 WindowInfo 解耦，便于在无 GUI 的测试里构造）
#[derive(Debug, Clone)]
pub struct Candidate {
    /// 运行期句柄，仅用于在本次恢复任务内去重（不写盘）
    pub hwnd: usize,
    pub pid: u32,
    pub process_path: Option<String>,
    pub process_name: String,
    pub class_name: String,
    pub title: String,
    pub rect: (i32, i32, i32, i32),
}

#[derive(Debug, Clone)]
pub struct MatchOutcome {
    pub picked: Option<usize>,
    /// 面向用户的识别依据说明（UI 的 "matchNote"）
    pub note: String,
    pub ambiguous: bool,
}

/// 接受门槛：必须是「可执行路径 + 窗口类名」同时匹配的量级，
/// 只靠标题相似绝不允许操作窗口（文档 §18.1 红线一）。
const ACCEPT: f64 = 0.70;
/// 前两名分差小于此值视为歧义 → 跳过，绝不猜
const AMBIGUITY_MARGIN: f64 = 0.08;

pub fn normalize_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for ch in title.chars() {
        if ch.is_alphanumeric() || ('\u{4E00}'..='\u{9FFF}').contains(&ch) {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        }
    }
    out
}

fn norm_path(p: &Option<String>) -> Option<String> {
    p.as_ref().map(|s| s.to_lowercase())
}

/// 标题相似度：完全一致给满分，互相包含给部分分。
/// 不引入编辑距离，避免"差一个字符"的窗口被当成同一个。
fn title_score(a: &str, b: &str) -> f64 {
    let na = normalize_title(a);
    let nb = normalize_title(b);
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 0.15;
    }
    if na.contains(&nb) || nb.contains(&na) {
        return 0.07;
    }
    0.0
}

fn rect_score(target: (i32, i32, i32, i32), cand: (i32, i32, i32, i32)) -> f64 {
    let area = (target.2 as f64) * (target.3 as f64);
    if area <= 0.0 {
        return 0.0;
    }
    let l = target.0.max(cand.0);
    let t = target.1.max(cand.1);
    let r = (target.0 + target.2).min(cand.0 + cand.2);
    let b = (target.1 + target.3).min(cand.1 + cand.3);
    if r <= l || b <= t {
        return 0.0;
    }
    let inter = ((r - l) as f64) * ((b - t) as f64);
    (inter / area).min(1.0) * 0.05
}

pub fn score_window(
    target: &SceneWindow,
    cand: &Candidate,
    target_rect: (i32, i32, i32, i32),
) -> f64 {
    let mut score = 0.0;

    match (norm_path(&target.process_path), norm_path(&cand.process_path)) {
        (Some(a), Some(b)) if a == b => score += 0.55,
        _ => {
            // 路径不可读（受保护进程）时退化为进程名匹配，权重明显更低
            if !target.process_name.is_empty()
                && target.process_name.eq_ignore_ascii_case(&cand.process_name)
            {
                score += 0.22;
            }
        }
    }

    if target.class_name == cand.class_name && !target.class_name.is_empty() {
        score += 0.25;
    }

    score += title_score(&target.title, &cand.title);
    score += rect_score(target_rect, cand.rect);

    score.min(1.0)
}

pub fn match_window(
    target: &SceneWindow,
    candidates: &[Candidate],
    target_rect: (i32, i32, i32, i32),
) -> MatchOutcome {
    let mut scored: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (i, score_window(target, c, target_rect)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let best = scored.first().copied();
    let second = scored.get(1).copied();

    match best {
        None => MatchOutcome {
            picked: None,
            note: "当前桌面没有同类窗口".into(),
            ambiguous: false,
        },
        Some((idx, score)) => {
            if score < ACCEPT {
                return MatchOutcome {
                    picked: None,
                    note: format!("匹配度不足（{score:.2}），不移动窗口"),
                    ambiguous: false,
                };
            }

            let ambiguous = second
                .map(|(_, s2)| s2 >= ACCEPT && (score - s2) < AMBIGUITY_MARGIN)
                .unwrap_or(false);

            let note = if ambiguous {
                let count = scored.iter().filter(|(_, s)| *s >= ACCEPT).count();
                format!("{count} 个候选窗口评分接近（{score:.2}），已跳过")
            } else {
                format!(
                    "按进程路径 + 窗口类名匹配（{score:.2}）{}",
                    if target.title.is_empty() {
                        String::new()
                    } else {
                        "，标题一致".to_string()
                    }
                )
            };

            MatchOutcome {
                picked: if ambiguous { None } else { Some(idx) },
                note,
                ambiguous,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::ShowState;

    fn target(title: &str, class: &str, path: &str) -> SceneWindow {
        SceneWindow {
            process_path: Some(path.into()),
            process_name: "notepad.exe".into(),
            class_name: class.into(),
            title: title.into(),
            monitor_key: "M1".into(),
            x_ratio: 0.0,
            y_ratio: 0.0,
            width_ratio: 0.5,
            height_ratio: 0.5,
            original_x: 0,
            original_y: 0,
            original_width: 800,
            original_height: 600,
            source_dpi: 96,
            show_state: ShowState::Normal,
            z_order: 0,
            reuse_policy: "reuse_or_launch".into(),
            launch_args: None,
            launch_priority: 0,
            launch_delay_ms: 0,
        }
    }

    fn cand(title: &str, class: &str, path: &str, rect: (i32, i32, i32, i32)) -> Candidate {
        Candidate {
            hwnd: 1,
            pid: 100,
            process_path: Some(path.into()),
            process_name: "notepad.exe".into(),
            class_name: class.into(),
            title: title.into(),
            rect,
        }
    }

    #[test]
    fn exact_identity_is_picked() {
        let t = target("未命名 - 记事本", "Notepad", r"C:\Windows\notepad.exe");
        let c = vec![cand(
            "未命名 - 记事本",
            "Notepad",
            r"C:\Windows\notepad.exe",
            (0, 0, 800, 600),
        )];
        let out = match_window(&t, &c, (0, 0, 800, 600));
        assert!(out.picked.is_some());
        assert!(!out.ambiguous);
    }

    #[test]
    fn two_identical_notepads_are_ambiguous_and_skipped() {
        let t = target("未命名 - 记事本", "Notepad", r"C:\Windows\notepad.exe");
        let c = vec![
            cand(
                "未命名 - 记事本",
                "Notepad",
                r"C:\Windows\notepad.exe",
                (0, 0, 800, 600),
            ),
            cand(
                "未命名 - 记事本",
                "Notepad",
                r"C:\Windows\notepad.exe",
                (0, 0, 800, 600),
            ),
        ];
        let out = match_window(&t, &c, (0, 0, 800, 600));
        assert!(
            out.picked.is_none(),
            "两个完全相同的窗口必须跳过而不是随便选一个"
        );
        assert!(out.ambiguous);
    }

    #[test]
    fn different_class_is_rejected_even_with_same_title() {
        let t = target("设置", "Notepad", r"C:\Windows\notepad.exe");
        let c = vec![cand(
            "设置",
            "ApplicationFrameWindow",
            r"C:\Windows\SystemApps\x.exe",
            (0, 0, 800, 600),
        )];
        let out = match_window(&t, &c, (0, 0, 800, 600));
        assert!(out.picked.is_none());
    }

    #[test]
    fn title_normalization_ignores_punctuation_and_case() {
        assert_eq!(normalize_title("Chrome — 新标签页"), "chrome新标签页");
        assert_eq!(
            normalize_title("*README.md - Visual Studio Code"),
            "readmemdvisualstudiocode"
        );
    }
}
