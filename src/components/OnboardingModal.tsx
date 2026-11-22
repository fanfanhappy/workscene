import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  ArrowRight,
  Boxes,
  Camera,
  CheckCircle2,
  Keyboard,
  LayoutGrid,
  Monitor,
  Moon,
  Play,
  ShieldCheck,
  Sparkles,
  Sun,
} from "lucide-react";
import { api } from "@/services/api";
import type { AppSettings } from "@/types/contract";
import { toast, useUiStore, type ThemeMode } from "@/stores/ui.store";
import { Button, Switch } from "@/components/ui";

const TOTAL_STEPS = 4;

/**
 * 首次安装欢迎动画与交互式上手指引（Apple macOS Setup Assistant 风格）：
 * - 首次打开自动播放入场动画，完成后持久化记录不再打扰；
 * - 随时可在「设置 → 关于」或 Ctrl+K 中重新观看。
 */
export function OnboardingModal() {
  const open = useUiStore((s) => s.onboardingOpen);
  const complete = useUiStore((s) => s.completeOnboarding);
  const themeMode = useUiStore((s) => s.themeMode);
  const setThemeMode = useUiStore((s) => s.setThemeMode);

  const navigate = useNavigate();
  const qc = useQueryClient();
  const [step, setStep] = useState(0);

  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
    enabled: open,
    staleTime: 10_000,
    retry: 0,
  });

  const saveSettings = useMutation({
    mutationFn: (next: AppSettings) => api.settingsSet(next),
    onSuccess: (saved) => {
      qc.setQueryData(["settings"], saved);
      qc.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (e) => toast.danger("保存偏好失败", String((e as Error).message)),
  });

  useEffect(() => {
    if (open) setStep(0);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        complete();
      } else if (e.key === "ArrowRight" && step < TOTAL_STEPS - 1) {
        setStep((s) => s + 1);
      } else if (e.key === "ArrowLeft" && step > 0) {
        setStep((s) => s - 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, step, complete]);

  if (!open) return null;

  const patchSetting = (partial: Partial<AppSettings>) => {
    if (!settings.data) return;
    saveSettings.mutate({ ...settings.data, ...partial });
  };

  const finishAndCapture = () => {
    complete();
    navigate("/capture");
  };

  const content = (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/48 p-4 backdrop-blur-md"
      role="dialog"
      aria-modal="true"
      aria-label="WorkScene 新手引导"
    >
      <div className="animate-dialog relative flex w-full max-w-[560px] flex-col overflow-hidden rounded-[18px] border border-line-strong/60 bg-surface shadow-overlay">
        {/* 顶部跳过按钮 */}
        <div className="flex items-center justify-between px-5 pt-4">
          <span className="inline-flex items-center gap-1.5 text-[11px] font-medium text-ink-3">
            <Sparkles size={12} className="text-accent" />
            WorkScene 新手向导 · {step + 1}/{TOTAL_STEPS}
          </span>
          <button
            type="button"
            onClick={complete}
            className="cursor-pointer rounded-full px-2.5 py-1 text-[11px] text-ink-3 transition-colors hover:bg-raised hover:text-ink"
          >
            跳过
          </button>
        </div>

        {/* 主体内容区：每步带平滑切换动画 */}
        <div key={step} className="animate-ob-step px-6 pb-5 pt-3">
          {step === 0 ? (
            <div className="flex flex-col items-center text-center">
              {/* Apple 风格动态桌面入场插画 */}
              <div className="relative mb-5 flex h-[168px] w-full max-w-[360px] items-center justify-center overflow-hidden rounded-[14px] border border-line bg-gradient-to-b from-accent/12 via-canvas/70 to-canvas p-4">
                {/* 背景柔光 */}
                <div
                  aria-hidden="true"
                  className="pointer-events-none absolute -top-10 size-44 rounded-full bg-accent/20 blur-3xl"
                />

                {/* 模拟多窗口自动归位动画 */}
                <div className="relative h-full w-full">
                  {/* 左主窗口（如 IDE / 文档） */}
                  <div className="animate-ob-win-1 absolute bottom-1 left-1 top-1 w-[56%] rounded-[9px] border border-accent/35 bg-surface/95 p-2 shadow-sm">
                    <div className="flex items-center gap-1">
                      <span className="size-1.5 rounded-full bg-[#ff5f57]" />
                      <span className="size-1.5 rounded-full bg-[#febc2e]" />
                      <span className="size-1.5 rounded-full bg-[#28c840]" />
                      <span className="ml-1.5 h-1.5 w-12 rounded-full bg-raised" />
                    </div>
                    <div className="mt-2.5 flex flex-col gap-1.5">
                      <div className="h-2 w-4/5 rounded-full bg-accent/22" />
                      <div className="h-1.5 w-3/5 rounded-full bg-raised" />
                      <div className="h-1.5 w-2/3 rounded-full bg-raised/70" />
                      <div className="mt-2 h-9 rounded-[6px] bg-canvas/80" />
                    </div>
                  </div>

                  {/* 右上窗口（如浏览器） */}
                  <div className="animate-ob-win-2 absolute right-1 top-1 h-[47%] w-[39%] rounded-[9px] border border-[#30d158]/40 bg-surface/95 p-2 shadow-sm">
                    <div className="flex items-center gap-1">
                      <span className="size-1.5 rounded-full bg-[#ff5f57]" />
                      <span className="size-1.5 rounded-full bg-[#febc2e]" />
                      <span className="size-1.5 rounded-full bg-[#28c840]" />
                    </div>
                    <div className="mt-2 h-2 w-3/4 rounded-full bg-[#30d158]/25" />
                    <div className="mt-1.5 h-1.5 w-1/2 rounded-full bg-raised" />
                  </div>

                  {/* 右下窗口（如终端） */}
                  <div className="animate-ob-win-3 absolute bottom-1 right-1 h-[43%] w-[39%] rounded-[9px] border border-[#af52de]/40 bg-surface/95 p-2 shadow-sm">
                    <div className="flex items-center gap-1">
                      <span className="size-1.5 rounded-full bg-[#af52de]" />
                      <span className="h-1.5 w-10 rounded-full bg-raised" />
                    </div>
                    <div className="mt-2 h-1.5 w-4/5 rounded-full bg-[#af52de]/25" />
                    <div className="mt-1 h-1.5 w-3/5 rounded-full bg-raised" />
                  </div>

                  {/* 中央浮动品牌徽标 */}
                  <div className="animate-ob-logo absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2">
                    <div className="animate-ob-float grid size-12 place-items-center rounded-[14px] bg-gradient-to-b from-[#2f9bff] to-[#0071e3] text-white shadow-[0_8px_22px_rgba(0,113,227,0.45)] ring-4 ring-surface">
                      <Boxes size={24} strokeWidth={2.1} />
                    </div>
                  </div>
                </div>
              </div>

              <h2 className="text-[20px] font-semibold tracking-tight text-ink">
                欢迎使用 WorkScene
              </h2>
              <p className="mt-1.5 max-w-[420px] text-body leading-relaxed text-ink-2">
                把你常用的软件、窗口摆法、文件夹和网页打包成「工作空间」，随时一键切换整套桌面现场。
              </p>

              <div className="mt-4 flex flex-wrap items-center justify-center gap-2 text-[11px] text-ink-2">
                <span className="inline-flex items-center gap-1 rounded-full border border-line bg-canvas/80 px-2.5 py-1">
                  <ShieldCheck size={12} className="text-success" /> 100% 本地离线
                </span>
                <span className="inline-flex items-center gap-1 rounded-full border border-line bg-canvas/80 px-2.5 py-1">
                  <ShieldCheck size={12} className="text-success" /> 不截屏 · 不读密码
                </span>
                <span className="inline-flex items-center gap-1 rounded-full border border-line bg-canvas/80 px-2.5 py-1">
                  <ShieldCheck size={12} className="text-success" /> 恢复前自动备份可撤销
                </span>
              </div>
            </div>
          ) : step === 1 ? (
            <div className="flex flex-col gap-3.5">
              <div className="text-center">
                <h2 className="text-[18px] font-semibold tracking-tight text-ink">
                  3 步上手，简单顺手
                </h2>
                <p className="mt-1 text-aux text-ink-3">
                  不需要复杂配置，像拍照一样记住你的工作现场
                </p>
              </div>

              <div className="grid gap-2.5">
                {[
                  {
                    num: "1",
                    icon: Camera,
                    color: "text-[#0071e3] bg-[#0071e3]/12",
                    title: "捕获当前桌面",
                    desc: "排好你常用的软件与浏览器，点「捕获现场」，毫秒级记住所有窗口位置、屏幕与最大化/最小化状态。",
                  },
                  {
                    num: "2",
                    icon: LayoutGrid,
                    color: "text-[#af52de] bg-[#af52de]/12",
                    title: "按需微调与挂载网页",
                    desc: "支持画布一键分屏排布（左右平分/四宫格）、批量粘贴网址或导入 Chrome/Edge 书签夹，以及设置优先启动代理。",
                  },
                  {
                    num: "3",
                    icon: Play,
                    color: "text-[#30d158] bg-[#30d158]/14",
                    title: "一键切换整套现场",
                    desc: "点「一键恢复」、右键系统托盘或直接按 Ctrl+Alt+1~9 快捷槽，自动拉起未开软件并精准摆回原位。",
                  },
                ].map((item) => {
                  const Icon = item.icon;
                  return (
                    <div
                      key={item.num}
                      className="flex items-start gap-3 rounded-[12px] border border-line bg-canvas/65 p-3"
                    >
                      <span
                        className={`grid size-9 shrink-0 place-items-center rounded-[10px] ${item.color}`}
                      >
                        <Icon size={17} />
                      </span>
                      <div className="min-w-0 flex-1">
                        <p className="text-body font-semibold text-ink">
                          {item.num}. {item.title}
                        </p>
                        <p className="mt-0.5 text-[11px] leading-relaxed text-ink-2">
                          {item.desc}
                        </p>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          ) : step === 2 ? (
            <div className="flex flex-col gap-3.5">
              <div className="text-center">
                <h2 className="text-[18px] font-semibold tracking-tight text-ink">
                  按你的习惯定制一下
                </h2>
                <p className="mt-1 text-aux text-ink-3">
                  所选配置立即生效，之后随时可在「设置」里修改
                </p>
              </div>

              {/* 主题选择 */}
              <div className="flex flex-col gap-1.5">
                <span className="text-aux font-medium text-ink">界面外观</span>
                <div className="grid grid-cols-3 gap-2">
                  {(
                    [
                      { id: "light", label: "浅色主题", icon: Sun },
                      { id: "dark", label: "深色主题", icon: Moon },
                      { id: "system", label: "跟随系统", icon: Monitor },
                    ] as { id: ThemeMode; label: string; icon: typeof Sun }[]
                  ).map(({ id, label, icon: Icon }) => {
                    const active = themeMode === id;
                    return (
                      <button
                        key={id}
                        type="button"
                        onClick={() => setThemeMode(id)}
                        className={`flex cursor-pointer flex-col items-center gap-1.5 rounded-[10px] border py-2.5 text-aux font-medium transition-all ${
                          active
                            ? "border-accent bg-accent/12 text-accent"
                            : "border-line bg-canvas/60 text-ink-2 hover:border-line-strong"
                        }`}
                      >
                        <Icon size={16} />
                        <span>{label}</span>
                      </button>
                    );
                  })}
                </div>
              </div>

              {/* 两个高频行为开关 */}
              <div className="flex flex-col gap-2.5 rounded-[12px] border border-line bg-canvas/60 p-3.5">
                <div className="flex items-center justify-between gap-4">
                  <div>
                    <p className="text-body font-medium text-ink">恢复时自动启动未打开的软件</p>
                    <p className="text-[11px] text-ink-3">
                      没运行的程序会自动帮你拉起并排好位置
                    </p>
                  </div>
                  <Switch
                    checked={settings.data?.restoreRelaunchDefault ?? true}
                    disabled={!settings.data}
                    onChange={(v) => patchSetting({ restoreRelaunchDefault: v })}
                    ariaLabel="恢复时自动启动未打开的软件"
                  />
                </div>

                <div className="flex items-center justify-between gap-4 border-t border-line/70 pt-2.5">
                  <div>
                    <p className="text-body font-medium text-ink">专注模式（收起无关窗口）</p>
                    <p className="text-[11px] text-ink-3">
                      切换工作空间时自动最小化非本空间窗口，绝不关闭软件
                    </p>
                  </div>
                  <Switch
                    checked={settings.data?.restoreFocusModeDefault ?? false}
                    disabled={!settings.data}
                    onChange={(v) => patchSetting({ restoreFocusModeDefault: v })}
                    ariaLabel="专注模式"
                  />
                </div>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center text-center">
              <div className="animate-ob-logo mb-3 grid size-14 place-items-center rounded-full bg-success/15 text-success">
                <CheckCircle2 size={30} strokeWidth={2.2} />
              </div>
              <h2 className="text-[19px] font-semibold tracking-tight text-ink">
                一切准备就绪！
              </h2>
              <p className="mt-1 max-w-[400px] text-aux leading-relaxed text-ink-2">
                记住这几个顺手的快捷键，日常切换桌面会比以前快十倍：
              </p>

              <div className="mt-4 grid w-full grid-cols-2 gap-2 text-left text-aux">
                {[
                  ["Ctrl + Alt + W", "随时唤起主窗口"],
                  ["Ctrl + Alt + 1~9", "秒切已绑定快捷槽"],
                  ["Ctrl + K", "聚焦搜索与命令"],
                  ["右键托盘图标", "不弹主窗一键恢复"],
                ].map(([k, desc]) => (
                  <div
                    key={k}
                    className="flex items-center justify-between gap-2 rounded-[9px] border border-line bg-canvas/70 px-3 py-2"
                  >
                    <span className="truncate text-[11px] text-ink-2">{desc}</span>
                    <kbd className="shrink-0 rounded-[5px] border border-line bg-surface px-1.5 py-0.5 font-mono text-[10px] text-ink">
                      {k}
                    </kbd>
                  </div>
                ))}
              </div>

              <div className="mt-4 flex items-center gap-1.5 text-[11px] text-ink-3">
                <Keyboard size={12} />
                关闭主窗口默认最小化到右下角系统托盘，右键托盘可退出。
              </div>
            </div>
          )}
        </div>

        {/* 底部进度点与操作按钮 */}
        <div className="flex items-center justify-between border-t border-line bg-canvas/55 px-5 py-3.5">
          {/* 进度指示点 */}
          <div className="flex items-center gap-1.5">
            {Array.from({ length: TOTAL_STEPS }).map((_, i) => (
              <button
                key={i}
                type="button"
                aria-label={`第 ${i + 1} 步`}
                onClick={() => setStep(i)}
                className={`h-1.5 cursor-pointer rounded-full transition-all duration-200 ${
                  i === step ? "w-5 bg-accent" : "w-1.5 bg-line-strong hover:bg-ink-3"
                }`}
              />
            ))}
          </div>

          <div className="flex items-center gap-2">
            {step > 0 ? (
              <Button size="sm" variant="secondary" onClick={() => setStep((s) => s - 1)}>
                <ArrowLeft size={13} /> 上一步
              </Button>
            ) : null}

            {step < TOTAL_STEPS - 1 ? (
              <Button size="sm" variant="primary" onClick={() => setStep((s) => s + 1)}>
                {step === 0 ? "开始体验" : "下一步"} <ArrowRight size={13} />
              </Button>
            ) : (
              <>
                <Button size="sm" variant="secondary" onClick={complete}>
                  先逛逛
                </Button>
                <Button size="sm" variant="primary" onClick={finishAndCapture}>
                  <Camera size={13} /> 捕获第一个空间
                </Button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );

  return typeof document !== "undefined" ? createPortal(content, document.body) : content;
}
