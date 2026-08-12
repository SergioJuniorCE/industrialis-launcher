import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "../lib/desktop";
import { DEFAULT_INSTANCE_SETTINGS, mergeInstanceSettings, type InstanceSettings } from "../lib/instance-settings";
import { InstanceSettingsTabs, type AccountOption, type JavaInfo } from "./InstanceSettingsTabs";

export function InstanceSettingsPanel({
  instanceId,
  packVersion,
  javaRefreshing,
  accounts,
  onOpenLauncherSettings,
  onRefreshJava,
  onSave,
}: {
  instanceId: string;
  packVersion: string;
  javaRefreshing: boolean;
  accounts: AccountOption[];
  onOpenLauncherSettings?: () => void;
  onRefreshJava: () => Promise<JavaInfo[]>;
  onSave: (instanceId: string, settings: InstanceSettings) => void;
}) {
  const [settings, setSettings] = useState<InstanceSettings>(DEFAULT_INSTANCE_SETTINGS);
  const [settingsTab, setSettingsTab] = useState("general");
  const [javaTestResult, setJavaTestResult] = useState<string | null>(null);
  const [testingJava, setTestingJava] = useState(false);
  const settingsRef = useRef(settings);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingSaveRef = useRef<{ id: string; settings: InstanceSettings } | null>(null);
  const loadVersionRef = useRef(0);
  const onSaveRef = useRef(onSave);

  useEffect(() => {
    onSaveRef.current = onSave;
  }, [onSave]);

  const flushPendingSave = useCallback(() => {
    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    const pending = pendingSaveRef.current;
    if (!pending) return;
    pendingSaveRef.current = null;
    onSaveRef.current(pending.id, pending.settings);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const loadVersion = ++loadVersionRef.current;
    flushPendingSave();
    invoke<InstanceSettings>("get_settings", { id: instanceId })
      .then((disk) => {
        if (cancelled || loadVersionRef.current !== loadVersion) return;
        const next = mergeInstanceSettings(disk);
        settingsRef.current = next;
        setSettings(next);
      })
      .catch(() => {
        if (cancelled || loadVersionRef.current !== loadVersion) return;
        const next = { ...DEFAULT_INSTANCE_SETTINGS };
        settingsRef.current = next;
        setSettings(next);
      });
    return () => {
      cancelled = true;
      flushPendingSave();
    };
  }, [flushPendingSave, instanceId]);

  const update = useCallback(
    (patch: Partial<InstanceSettings>) => {
      loadVersionRef.current += 1;
      const next = { ...settingsRef.current, ...patch };
      settingsRef.current = next;
      setSettings(next);
      pendingSaveRef.current = { id: instanceId, settings: next };
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
      saveTimerRef.current = setTimeout(flushPendingSave, 250);
    },
    [flushPendingSave, instanceId],
  );

  const browseJava = async () => {
    const picked = await invoke<string | null>("browse_java_executable");
    if (picked) update({ java_path: picked });
  };

  const detectJava = async () => {
    const detected = await onRefreshJava();
    if (detected.length > 0) update({ java_path: detected[0].path });
  };

  const testJava = async () => {
    setTestingJava(true);
    setJavaTestResult(null);
    try {
      const path = settings.java_path?.trim() || null;
      const result = await invoke<string>("test_java", { javaPath: path });
      setJavaTestResult(result);
    } catch (e) {
      setJavaTestResult(`Failed: ${e}`);
    } finally {
      setTestingJava(false);
    }
  };

  return (
    <InstanceSettingsTabs
      settings={settings}
      packVersion={packVersion}
      settingsTab={settingsTab}
      accounts={accounts}
      javaRefreshing={javaRefreshing}
      javaTestResult={javaTestResult}
      testingJava={testingJava}
      onSettingsTabChange={setSettingsTab}
      onUpdate={update}
      onDetectJava={() => void detectJava()}
      onBrowseJava={() => void browseJava()}
      onTestJava={() => void testJava()}
      onOpenLauncherSettings={onOpenLauncherSettings}
    />
  );
}
