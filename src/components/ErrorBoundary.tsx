import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, Home, RefreshCw } from "lucide-react";
import { Button, Card } from "@/components/ui";

interface State {
  error: Error | null;
}

/** 全局 React 错误边界：避免单个页面异常导致整个桌面窗口白屏 */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("WorkScene UI ErrorBoundary:", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="flex h-full items-center justify-center bg-canvas p-6">
        <Card className="flex max-w-md flex-col items-center gap-3 p-6 text-center">
          <div className="grid size-11 place-items-center rounded-full bg-danger/12 text-danger">
            <AlertTriangle size={20} />
          </div>
          <div>
            <h2 className="text-card font-semibold text-ink">页面遇到意外错误</h2>
            <p className="mt-1 text-aux text-ink-3">
              你的工作空间与备份数据完好无损，可尝试返回首页或重新加载界面。
            </p>
          </div>
          <code className="selectable max-h-24 w-full overflow-auto rounded-[8px] border border-line bg-canvas px-3 py-2 text-left font-mono text-[11px] text-danger">
            {this.state.error.message || "未知渲染错误"}
          </code>
          <div className="mt-1 flex items-center gap-2">
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                this.setState({ error: null });
                window.location.hash = "#/";
              }}
            >
              <Home size={13} /> 返回总览
            </Button>
            <Button
              size="sm"
              variant="primary"
              onClick={() => window.location.reload()}
            >
              <RefreshCw size={13} /> 重新加载
            </Button>
          </div>
        </Card>
      </div>
    );
  }
}
