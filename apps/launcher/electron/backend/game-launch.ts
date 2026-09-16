import fs from "node:fs/promises";
import path from "node:path";
import { existsSync } from "node:fs";
import { ensureFreshToken, loadAccounts } from "./auth";
import {
  buildClasspath,
  buildLaunchConfig,
  expandMinecraftArguments,
  instanceCommandVars,
  runShellCommand,
  splitCommandArgs,
  substituteCommandVars,
  syncAssets,
  writeLaunchArgfile,
} from "./launch";
import { javaGuiExecutable, javaPath } from "./java";
import { applyPersistentCustomMods, flattenNestedPack, prepareInstanceConfigs } from "./pack";
import { applyPersistentMinecraft } from "./minecraft-files";
import { instanceDir, sanitizeName } from "./paths";
import { loadInstanceSettings, loadLauncherSettings, saveInstanceSettings } from "./settings";
import { killGameProcess, spawnGameProcess, waitForGameProcess } from "./process-manager";
import { exists } from "./fs-utils";
import type { AccountData, InstanceSettings } from "./types";
import type { BackendContext } from "./backend-context";

export async function killInstance(ctx: BackendContext, rawId: string): Promise<void> {
  const id = sanitizeName(rawId);
  const running = ctx.state.running.get(id);
  if (running) await killGameProcess(running);
}

export async function launchInstance(ctx: BackendContext, rawId: string): Promise<void> {
  const id = sanitizeName(rawId.trim());
  if (ctx.state.running.has(id)) throw new Error("Instance is already running");
  if (ctx.state.updateInProgress.has(id)) throw new Error("pack update in progress for this instance");
  if (ctx.state.reinstallInProgress.has(id)) throw new Error("clean reinstall in progress for this instance");
  const instance = instanceDir(id);
  if (!(await exists(instance))) throw new Error("instance not installed");
  await flattenNestedPack(instance);
  await prepareInstanceConfigs(instance, false);
  await applyPersistentCustomMods(instance);
  await applyPersistentMinecraft(instance);
  const [settings, launcherSettings] = await Promise.all([loadInstanceSettings(id), loadLauncherSettings()]);
  const java = resolveJava(settings, launcherSettings.default_java_path);
  const config = await buildLaunchConfig(instance, id, (entryId, stream, line) => ctx.emitLog(entryId, stream, line));
  await syncAssets(config, id, (entryId, stream, line) => ctx.emitLog(entryId, stream, line));
  const classpath = buildClasspath(config.libraries);
  const vars = instanceCommandVars(id, settings.name, instance, java);
  if (settings.override_commands && settings.pre_launch_command.trim()) {
    await runShellCommand(substituteCommandVars(settings.pre_launch_command.trim(), vars), instance, settings.override_env ? settings.env_vars : {});
    ctx.emitLog(id, "system", "Pre-launch command finished");
  }
  const args = [`-Xms${settings.override_memory ? settings.min_ram_mb : 4096}M`, `-Xmx${settings.override_memory ? settings.max_ram_mb : 6144}M`];
  if (settings.override_memory && settings.perm_gen_mb > 0) args.push(`-XX:PermSize=${settings.perm_gen_mb}M`, `-XX:MaxPermSize=${settings.perm_gen_mb}M`);
  if (settings.override_java_args && settings.jvm_args.trim()) args.push(...splitCommandArgs(settings.jvm_args));
  args.push("-cp", classpath, ...config.jvmArgs, config.mainClass, ...config.programArgs);
  if (settings.override_window && !settings.launch_maximized) args.push("--width", String(settings.window_width), "--height", String(settings.window_height));
  if (settings.join_server_on_launch && settings.join_server_address.trim()) args.push("--server", settings.join_server_address.trim());
  const accounts = await loadAccounts();
  const account = chooseAccount(settings, accounts, launcherSettings.default_account_id);
  const auth = await launchAuth(account);
  if (config.minecraftArgumentsTemplate)
    args.push(
      ...expandMinecraftArguments(config.minecraftArgumentsTemplate, {
        auth_player_name: auth.username,
        version_name: config.minecraftVersion,
        game_directory: config.gameDir,
        assets_root: config.assetsDir,
        assets_index_name: config.assetIndex?.id ?? config.minecraftVersion,
        auth_uuid: auth.uuid,
        auth_access_token: auth.accessToken,
        user_properties: "{}",
        user_type: auth.userType,
      }),
    );
  else
    args.push(
      "--username",
      auth.username,
      "--version",
      config.minecraftVersion,
      "--gameDir",
      config.gameDir,
      "--assetsDir",
      config.assetsDir,
      "--accessToken",
      auth.accessToken,
      "--uuid",
      auth.uuid,
      "--userType",
      auth.userType,
    );
  ctx.emitLog(id, "system", "──────── Launch ────────");
  ctx.emitLog(id, "system", `Java: ${java}`);
  ctx.emitLog(id, "system", `Main class: ${config.mainClass}`);
  ctx.emitLog(id, "system", `Classpath: ${config.libraries.length} libraries (${classpath.length} chars)`);
  await writeLaunchArgfile(path.join(instance, "launch.arg"), args);
  ctx.emitLog(id, "system", `Launch args saved to ${path.join(instance, "launch.arg")}`);
  const launchJava = javaGuiExecutable(java);
  const command = resolveLaunchCommand(settings, launchJava, args, vars);
  const gameDir = config.gameDir;
  await fs.mkdir(gameDir, { recursive: true });
  const started = Date.now();
  const running = await spawnGameProcess(command.executable, command.args, gameDir, settings.override_env ? settings.env_vars : {}, (stream, line) => {
    for (const part of line.split(/\r?\n/u)) if (part) ctx.emitLog(id, stream, part);
  });
  ctx.state.running.set(id, running);
  await ctx.persistRunningProcesses().catch((error) => {
    ctx.emitLog(id, "system", `Could not remember game process ${running.pid}: ${String(error)}`);
  });
  ctx.emit("instance-started", { id });
  const exitCode = await waitForGameProcess(running);
  ctx.state.running.delete(id);
  await ctx.persistRunningProcesses().catch((error) => {
    ctx.emitLog(id, "system", `Could not clear remembered game process: ${String(error)}`);
  });
  ctx.emitLog(id, "system", `Process exited with code ${exitCode}`);
  try {
    if (settings.override_game_time && settings.record_game_time) {
      settings.total_play_seconds += Math.floor((Date.now() - started) / 1000);
      await saveInstanceSettings(id, settings);
    }
    if (settings.override_commands && settings.post_exit_command.trim()) {
      await runShellCommand(substituteCommandVars(settings.post_exit_command.trim(), vars), instance, settings.override_env ? settings.env_vars : {});
      ctx.emitLog(id, "system", "Post-exit command finished");
    }
  } finally {
    await ctx.flushConsoleLog(id);
    ctx.emit("instance-stopped", { id, exit_code: exitCode });
  }
  if (exitCode !== 0) throw new Error(`game exited with code ${exitCode}`);
}

async function launchAuth(account: AccountData): Promise<{ username: string; uuid: string; accessToken: string; userType: string }> {
  if (account.account_type === "offline") {
    const username = account.minecraft_profile?.name ?? "";
    const uuid = account.minecraft_profile?.id ?? "";
    if (!username || !uuid) throw new Error("Offline account is missing a username.");
    return { username, uuid, accessToken: "0", userType: "legacy" };
  }
  if (account.minecraft_entitlement && !account.minecraft_entitlement.can_play_minecraft)
    throw new Error("This Microsoft account does not own Minecraft Java Edition.");
  const token = await ensureFreshToken(account);
  const username = account.minecraft_profile?.name ?? "";
  const uuid = account.minecraft_profile?.id ?? "";
  if (!username || !uuid) throw new Error("This Microsoft account has no Minecraft profile yet. Set a username in the official launcher first.");
  return { username, uuid, accessToken: token, userType: "msa" };
}

function resolveJava(settings: InstanceSettings, defaultPath: string | null): string {
  if (settings.override_java_location && settings.java_path?.trim()) {
    if (!existsSync(settings.java_path)) throw new Error(`configured Java not found: ${settings.java_path}`);
    return settings.java_path;
  }
  if (defaultPath?.trim()) {
    if (!existsSync(defaultPath)) throw new Error(`default Java not found: ${defaultPath}`);
    return defaultPath;
  }
  const detected = javaPath();
  if (!detected)
    throw new Error("no Java configured or found — choose a default Java in launcher settings, set JAVA_HOME, or pick a Java in instance settings");
  return detected;
}

function chooseAccount(settings: InstanceSettings, accounts: AccountData[], defaultAccountId: string | null): AccountData {
  if (!accounts.length) throw new Error("Add an account in Accounts before launching.");
  if (settings.override_account && settings.account_id) {
    const account = accounts.find((entry) => entry.id === settings.account_id);
    if (!account) throw new Error("The account selected in instance settings was not found.");
    return account;
  }
  if (defaultAccountId) {
    const account = accounts.find((entry) => entry.id === defaultAccountId);
    if (account) return account;
  }
  if (accounts.length === 1) return accounts[0];
  throw new Error("Set a default account in Accounts before launching.");
}

function resolveLaunchCommand(settings: InstanceSettings, java: string, args: string[], vars: Record<string, string>): { executable: string; args: string[] } {
  if (!settings.override_commands || !settings.wrapper_command.trim()) return { executable: java, args };
  const wrapper = substituteCommandVars(settings.wrapper_command.trim(), vars);
  const parts = splitCommandArgs(wrapper);
  const executable = parts.shift();
  if (!executable) throw new Error("wrapper command is empty");
  return { executable, args: [...parts, java, ...args] };
}
