import type { ItemKind } from "./api";

/**
 * Audio cues, so a new row can be identified without looking at the screen.
 * Each kind gets its own interval, which is far easier to tell apart than
 * differences in pitch alone.
 */
const CUES: Record<ItemKind, number[]> = {
  permission: [880, 1174], // rising fourth — needs a decision
  needsInput: [988, 740], // falling — a question, answered in the editor
  completed: [587, 784, 1046], // arpeggio — nothing to do
};

let context: AudioContext | null = null;
let lastPlayed = 0;

/**
 * Several sessions finishing at once would otherwise stack their cues into
 * noise, and a burst carries no more information than one chime does.
 */
const COOLDOWN_MS = 450;

export function play(kind: ItemKind) {
  const now = Date.now();
  if (now - lastPlayed < COOLDOWN_MS) return;
  lastPlayed = now;

  try {
    context ??= new AudioContext();
    // Autoplay policy leaves the context suspended until the first gesture.
    if (context.state === "suspended") void context.resume();

    const start = context.currentTime;
    CUES[kind].forEach((frequency, index) => {
      const at = start + index * 0.09;
      const osc = context!.createOscillator();
      const gain = context!.createGain();
      osc.type = "sine";
      osc.frequency.value = frequency;
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(0.12, at + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.16);
      osc.connect(gain).connect(context!.destination);
      osc.start(at);
      osc.stop(at + 0.18);
    });
  } catch {
    // Sound is an enhancement; a machine without an output device is fine.
  }
}
