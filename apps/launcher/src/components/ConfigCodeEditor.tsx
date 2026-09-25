import CodeMirror from "@uiw/react-codemirror";
import { cpp } from "@codemirror/lang-cpp";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView } from "@codemirror/view";
import { cn } from "../lib/utils";

interface ConfigCodeEditorProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  isDark?: boolean;
  className?: string;
  lineWrapping?: boolean;
}

export function ConfigCodeEditor({ value, onChange, disabled = false, isDark = true, className, lineWrapping = true }: ConfigCodeEditorProps) {
  return (
    <CodeMirror
      className={cn(
        "config-code-editor h-full min-h-0 text-xs leading-relaxed",
        !isDark && "config-code-editor-light",
        !lineWrapping && "config-code-editor-no-wrap",
        className,
      )}
      value={value}
      height="100%"
      theme={isDark ? oneDark : "light"}
      extensions={[cpp(), ...(lineWrapping ? [EditorView.lineWrapping] : [])]}
      editable={!disabled}
      basicSetup={{
        lineNumbers: true,
        foldGutter: true,
        highlightActiveLine: true,
        bracketMatching: true,
        autocompletion: false,
      }}
      onChange={(next) => onChange(next)}
    />
  );
}
