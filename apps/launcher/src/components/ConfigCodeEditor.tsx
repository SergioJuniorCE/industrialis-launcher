import CodeMirror from "@uiw/react-codemirror";
import { cpp } from "@codemirror/lang-cpp";
import { oneDark } from "@codemirror/theme-one-dark";
import { cn } from "../lib/utils";

interface ConfigCodeEditorProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  isDark?: boolean;
  className?: string;
}

export function ConfigCodeEditor({
  value,
  onChange,
  disabled = false,
  isDark = true,
  className,
}: ConfigCodeEditorProps) {
  return (
    <CodeMirror
      className={cn(
        "config-code-editor h-full min-h-0 text-[11px] leading-relaxed",
        !isDark && "config-code-editor-light",
        className,
      )}
      value={value}
      height="100%"
      theme={isDark ? oneDark : "light"}
      extensions={[cpp()]}
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