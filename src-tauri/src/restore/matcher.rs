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

    let exact_path = match (
        norm_path(&target.process_path),
        norm_path(&cand.process_path),
    ) {
        (Some(a), Some(b)) if a == b => {
            score += 0.55;
            true
        }
        _ => {
            // 路径不可读（受保护进程）时退化为进程名匹配。
            // 0.30 + 类名 0.25 + 标题 0.15 + 位置 0.05 = 0.75，必须达到 ACCEPT；
            // 旧值 0.22 的上限只有 0.67，等于让这条回退分支永远失效。
            if !target.process_name.is_empty()
                && target.process_name.eq_ignore_ascii_case(&cand.process_name)
            {
                score += 0.30;
            }
            false
        }
    };

    if !target.class_name.is_empty() {
        if target.class_name == cand.class_name {
            score += 0.25;
        }
    } else if exact_path {
        // 手动添加的 .exe 应用尚未捕获过 class_name：在进程路径 100% 精确一致时允许直接匹配排版
        score += 0.25;
    }

    score += title_score(&target.title, &cand.title);
    score += rect_score(target_rect, cand.rect);

    score.min(1.0)
}

/// 计算两个矩形中心点的曼哈顿距离，用于在同应用多窗口评分相近时做稳定就近配对
fn center_distance(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> i64 {
    let ax = a.0 as i64 + (a.2 as i64) / 2;
    let ay = a.1 as i64 + (a.3 as i64) / 2;
    let bx = b.0 as i64 + (b.2 as i64) / 2;
    let by = b.1 as i64 + (b.3 as i64) / 2;
    (ax - bx).abs() + (ay - by).abs()
}

/// 检查两个候选窗口是否属于同一个可执行文件与窗口类（同应用多实例）
fn is_same_app_identity(a: &Candidate, b: &Candidate) -> bool {
    let same_path = match (norm_path(&a.process_path), norm_path(&b.process_path)) {
        (Some(pa), Some(pb)) => pa == pb,
        _ => false,
    };
    same_path && !a.class_name.is_empty() && a.class_name == b.class_name
}

pub fn match_window(
    target: &SceneWindow,
    candidates: &[Candidate],
    target_rect: (i32, i32, i32, i32),
) -> MatchOutcome {
    let mut scored: Vec<(usize, f64, i64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                i,
                score_window(target, c, target_rect),
                center_distance(target_rect, c.rect),
            )
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.cmp(&b.0))
    });

    let best = scored.first().copied();
    let second = scored.get(1).copied();
    let describe = |entry: Option<(usize, f64, i64)>| {
        entry.map(|(i, score, _)| {
            let c = &candidates[i];
            format!(
                "{score:.3} pid={} name={} class={} path={:?} title={:?}",
                c.pid, c.process_name, c.class_name, c.process_path, c.title
            )
        })
    };
    tracing::debug!(
        target_title = %target.title,
        target_process = %target.process_name,
        target_path = ?target.process_path,
        target_class = %target.class_name,
        candidates = candidates.len(),
        best = ?describe(best),
        second = ?describe(second),
        "window candidates scored"
    );

    match best {
        None => MatchOutcome {
            picked: None,
            note: "当前桌面没有同类窗口".into(),
            ambiguous: false,
        },
        Some((idx, score, _)) => {
            if score < ACCEPT {
                // 应用升级/主题切换后窗口类名、标题会变，但 exe 路径不变。
                // 如果该 exe 在当前桌面只有一个窗口，按路径唯一匹配仍比直接跳过可靠；
                // 多个同 exe 窗口时仍交给上面的评分/歧义逻辑，绝不乱挑。
                if let Some(target_path) = norm_path(&target.process_path) {
                    let same_path: Vec<usize> = candidates
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| {
                            norm_path(&c.process_path).as_deref() == Some(target_path.as_str())
                        })
                        .map(|(i, _)| i)
                        .collect();
                    if let [only] = same_path.as_slice() {
                        let path_score = score_window(target, &candidates[*only], target_rect);
                        tracing::info!(
                            target = %target.title,
                            process = %target.process_name,
                            path = %target_path,
                            score = path_score,
                            "window class/title changed; using unique executable-path match"
                        );
                        return MatchOutcome {
                            picked: Some(*only),
                            note: format!(
                                "按进程路径唯一匹配（{path_score:.2}，窗口类或标题已变化）"
                            ),
                            ambiguous: false,
                        };
                    }
                }
                return MatchOutcome {
                    picked: None,
                    note: format!("匹配度不足（{score:.2}），不移动窗口"),
                    ambiguous: false,
                };
            }

            // 同应用多实例（路径 + 类名完全一致，例如多个 Chrome / VS Code / 终端）：
            // 允许按评分 + 就近距离 1 对 1 稳定分配，避免多窗口全部触发歧义死锁。
            // 仅当两个高分候选属于不同应用或弱识别（无路径退化）且分差极小时才判歧义。
            let ambiguous = second
                .map(|(idx2, s2, _)| {
                    s2 >= ACCEPT
                        && (score - s2) < AMBIGUITY_MARGIN
                        && !is_same_app_identity(&candidates[idx], &candidates[idx2])
                })
                .unwrap_or(false);

            let multi_same_app = second
                .map(|(idx2, s2, _)| {
                    s2 >= ACCEPT && is_same_app_identity(&candidates[idx], &candidates[idx2])
                })
                .unwrap_or(false);

            let note = if ambiguous {
                let count = scored.iter().filter(|(_, s, _)| *s >= ACCEPT).count();
                format!("{count} 个候选窗口评分接近（{score:.2}），已跳过")
            } else if multi_same_app {
                format!("多实例按标题与就近位置分配（{score:.2}）")
            } else {
                let t_hit = title_score(&target.title, &candidates[idx].title) > 0.0;
                format!(
                    "按进程路径 + 窗口类名匹配（{score:.2}）{}",
                    if t_hit { "，标题匹配" } else { "" }
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

/// 用于「收工」关闭窗口的严格匹配：除了路径+类名命中外，若目标有标题，
/// 必须满足标题相似或位置高度重合，避免误关同软件下不相关的窗口。
pub fn match_window_for_close(
    target: &SceneWindow,
    candidates: &[Candidate],
    target_rect: (i32, i32, i32, i32),
) -> Option<usize> {
    let mut scored: Vec<(usize, f64, f64, i64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let s = score_window(target, c, target_rect);
            let ts = title_score(&target.title, &c.title);
            let rs = rect_score(target_rect, c.rect);
            (i, s, ts + rs, center_distance(target_rect, c.rect))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.3.cmp(&b.3))
    });

    let (idx, score, extra, _) = scored.first().copied()?;
    if score < ACCEPT {
        return None;
    }
    // 要求标题相似（>=0.07）或位置明显重合（>=0.02），防止仅凭 exe 路径误关无关窗口
    if extra >= 0.02 || target.title.trim().is_empty() {
        Some(idx)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::ShowState;

    fn target(title: &str, class: &str, path: &str) -> SceneWindow {
        SceneWindow {
            process_path: Some(path.into()),
            browser_tabs: Vec::new(),
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
    fn unique_executable_path_matches_when_class_and_title_changed() {
        let t = target("宿舍主机", "FlutterMultiWindow", r"D:\AweSun\flutter\AweSun.exe");
        let c = cand(
            "向日葵远程控制",
            "FLUTTER_RUNNER_WIN32_WINDOW",
            r"D:\AweSun\flutter\AweSun.exe",
            (0, 0, 1000, 640),
        );
        let out = match_window(&t, &[c], (0, 0, 1000, 640));
        assert!(out.picked.is_some(), "唯一 exe 应能兜底匹配：{}", out.note);
        assert!(out.note.contains("路径唯一"));
    }

    #[test]
    fn unreadable_path_falls_back_to_process_name_and_still_matches() {
        let mut t = target(
            "项目 - 编辑器",
            "Chrome_WidgetWin_1",
            r"C:\Chrome\chrome.exe",
        );
        t.process_path = None;
        let mut c = cand(
            "项目 - 编辑器",
            "Chrome_WidgetWin_1",
            r"C:\Chrome\chrome.exe",
            (0, 0, 800, 600),
        );
        c.process_path = None;
        let out = match_window(&t, &[c], (0, 0, 800, 600));
        assert!(
            out.picked.is_some(),
            "路径不可读时应能按进程名+类名+标题回退匹配：{}",
            out.note
        );
    }

    #[test]
    fn two_identical_notepads_pick_nearest_instead_of_deadlocking() {
        let t = target("未命名 - 记事本", "Notepad", r"C:\Windows\notepad.exe");
        let c = vec![
            cand(
                "未命名 - 记事本",
                "Notepad",
                r"C:\Windows\notepad.exe",
                (900, 0, 800, 600),
            ),
            cand(
                "未命名 - 记事本",
                "Notepad",
                r"C:\Windows\notepad.exe",
                (10, 10, 800, 600),
            ),
        ];
        let out = match_window(&t, &c, (0, 0, 800, 600));
        assert_eq!(
            out.picked,
            Some(1),
            "同应用多实例应优先选位置最近的窗口，不再歧义死锁"
        );
        assert!(!out.ambiguous);
    }

    #[test]
    fn close_matcher_rejects_unrelated_window_by_title_and_position() {
        let t = target(
            "项目 A - Chrome",
            "Chrome_WidgetWin_1",
            r"C:\Chrome\chrome.exe",
        );
        let c = vec![cand(
            "哔哩哔哩 - Chrome",
            "Chrome_WidgetWin_1",
            r"C:\Chrome\chrome.exe",
            (1000, 500, 800, 600),
        )];
        assert!(
            match_window_for_close(&t, &c, (0, 0, 800, 600)).is_none(),
            "收工不应误关标题和位置都无关的同软件窗口"
        );
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
