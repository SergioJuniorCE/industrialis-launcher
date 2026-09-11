type AudioContextConstructor = new () => AudioContext;

let completionAudioContext: AudioContext | null = null;

function getAudioContext(): AudioContext | null {
  if (typeof window === "undefined") return null;
  const audioContextConstructor = window.AudioContext ?? (window as Window & { webkitAudioContext?: AudioContextConstructor }).webkitAudioContext;
  if (!audioContextConstructor) return null;

  try {
    completionAudioContext ??= new audioContextConstructor();
    return completionAudioContext;
  } catch {
    return null;
  }
}

export function prepareCompletionSound(): void {
  const context = getAudioContext();
  if (context?.state !== "suspended") return;
  try {
    void context.resume().catch(() => undefined);
  } catch {
    // Audio is an optional notification and must not affect the copy operation.
  }
}

function scheduleCompletionSound(context: AudioContext): void {
  if (context.state === "closed") return;

  const now = context.currentTime;
  const oscillator = context.createOscillator();
  const gain = context.createGain();
  oscillator.type = "sine";
  oscillator.frequency.setValueAtTime(740, now);
  oscillator.frequency.setValueAtTime(988, now + 0.1);
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(0.12, now + 0.015);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.3);
  oscillator.connect(gain);
  gain.connect(context.destination);
  oscillator.start(now);
  oscillator.stop(now + 0.3);
}

export function playCompletionSound(): void {
  const context = getAudioContext();
  if (!context) return;
  if (context.state === "suspended") {
    void context
      .resume()
      .then(() => scheduleCompletionSound(context))
      .catch(() => undefined);
    return;
  }

  try {
    scheduleCompletionSound(context);
  } catch {
    // Audio is an optional notification and must not affect the completed operation.
  }
}
