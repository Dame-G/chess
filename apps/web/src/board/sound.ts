import { stored } from "../stored";

/**
 * The sounds of a move, of a game ending and of a clock running low, made with WebAudio from a burst of noise and a few
 * sine notes, so there is nothing to bundle or license. They are quiet by default and can be muted in Settings.
 */
export type MoveSound = "move" | "capture" | "check" | "start" | "end" | "low";

/** Whether the person asked the system for less motion: sounds then start muted until they are switched on. */
const reducedMotion = (): boolean =>
  typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Whether sounds play. Remembered in this browser once chosen; until then it follows the reduced-motion preference. */
export const soundOn = stored<boolean>("sound.on", !reducedMotion());
/** How loud, from 0 to 1. The default is low: the app is something you sit with for an hour. */
export const soundVolume = stored<number>("sound.volume", 0.5);

let context: AudioContext | undefined;
let master: GainNode | undefined;
let noise: AudioBuffer | undefined;
let last = 0;

/** The master volume every sound goes through, set when a sound plays. */
const out = (ctx: AudioContext): AudioNode => master ?? ctx.destination;

/** What a move sounds like, from its SAN. */
export function soundOf(san: string | undefined): MoveSound {
  if (!san) return "move";
  if (san.includes("+") || san.includes("#")) return "check";
  return san.includes("x") ? "capture" : "move";
}

function ready(): AudioContext | undefined {
  if (context) return context;
  const Ctor =
    window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctor) return undefined;
  context = new Ctor();
  master = context.createGain();
  master.connect(context.destination);
  // A short burst of white noise, shaped below into the knock of a piece on wood.
  noise = context.createBuffer(1, Math.floor(context.sampleRate * 0.12), context.sampleRate);
  const data = noise.getChannelData(0);
  for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
  return context;
}

function knock(ctx: AudioContext, at: number, gain: number, pitch: number, length: number) {
  const src = ctx.createBufferSource();
  src.buffer = noise!;
  const filter = ctx.createBiquadFilter();
  filter.type = "bandpass";
  filter.frequency.value = pitch;
  filter.Q.value = 0.9;
  const env = ctx.createGain();
  env.gain.setValueAtTime(0.0001, at);
  env.gain.exponentialRampToValueAtTime(gain, at + 0.004);
  env.gain.exponentialRampToValueAtTime(0.0001, at + length);
  src.connect(filter).connect(env).connect(out(ctx));
  src.start(at);
  src.stop(at + length + 0.02);
  // The body of the knock: a short low note falling away.
  const body = ctx.createOscillator();
  const bodyEnv = ctx.createGain();
  body.frequency.setValueAtTime(pitch / 4, at);
  body.frequency.exponentialRampToValueAtTime(pitch / 8, at + length);
  bodyEnv.gain.setValueAtTime(0.0001, at);
  bodyEnv.gain.exponentialRampToValueAtTime(gain * 0.9, at + 0.004);
  bodyEnv.gain.exponentialRampToValueAtTime(0.0001, at + length);
  body.connect(bodyEnv).connect(out(ctx));
  body.start(at);
  body.stop(at + length + 0.02);
}

function ping(ctx: AudioContext, at: number) {
  for (const [i, hz] of [880, 1318].entries()) {
    const osc = ctx.createOscillator();
    const env = ctx.createGain();
    const t = at + i * 0.07;
    osc.type = "sine";
    osc.frequency.value = hz;
    env.gain.setValueAtTime(0.0001, t);
    env.gain.exponentialRampToValueAtTime(0.08, t + 0.008);
    env.gain.exponentialRampToValueAtTime(0.0001, t + 0.16);
    osc.connect(env).connect(out(ctx));
    osc.start(t);
    osc.stop(t + 0.18);
  }
}

/** The end of a game: two soft notes, the second a fourth lower, so it reads as "that is over" and not as an alert. */
function chime(ctx: AudioContext, at: number) {
  for (const [i, hz] of [659, 494].entries()) {
    const osc = ctx.createOscillator();
    const env = ctx.createGain();
    const t = at + i * 0.16;
    osc.type = "triangle";
    osc.frequency.value = hz;
    env.gain.setValueAtTime(0.0001, t);
    env.gain.exponentialRampToValueAtTime(0.12, t + 0.012);
    env.gain.exponentialRampToValueAtTime(0.0001, t + 0.45);
    osc.connect(env).connect(out(ctx));
    osc.start(t);
    osc.stop(t + 0.5);
  }
}

/** Plays one, unless sounds are off. Steps that come faster than the ear can separate are dropped, so held keys do not buzz. */
export function playMoveSound(kind: MoveSound) {
  if (!soundOn.value) return;
  const now = performance.now();
  if (now - last < 90) return;
  last = now;
  const ctx = ready();
  if (!ctx) return;
  if (ctx.state === "suspended") void ctx.resume();
  if (master) master.gain.value = Math.min(1, Math.max(0, soundVolume.value));
  const at = ctx.currentTime + 0.005;
  if (kind === "start") {
    ping(ctx, at);
  } else if (kind === "end") {
    chime(ctx, at);
  } else if (kind === "low") {
    // Two short, high ticks: a clock running out.
    knock(ctx, at, 0.25, 1800, 0.04);
    knock(ctx, at + 0.12, 0.25, 1800, 0.04);
  } else if (kind === "capture") {
    knock(ctx, at, 0.5, 900, 0.1);
    knock(ctx, at + 0.035, 0.3, 600, 0.09);
  } else {
    knock(ctx, at, 0.32, 700, 0.07);
    if (kind === "check") ping(ctx, at + 0.05);
  }
}
