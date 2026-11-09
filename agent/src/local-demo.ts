/**
 * The whole Franchise story on a local NEAR node, no testnet account needed:
 * mint an agent, register its operator key, a client hires it, the runtime answers
 * and gets paid, the NFT is sold, the seller's key dies, the buyer takes over and
 * withdraws everything.
 *
 *   ../scripts/build.sh   # once, builds res/franchise.wasm
 *   npm run demo:local            # run the story and exit
 *   npm run demo:local -- --keep  # also seed more agents and keep the node running
 *                                 # (point web/ at it with VITE_RPC_URL / VITE_CONTRACT)
 *
 * Set NEARAI_API_KEY to have a private model on NEAR AI Cloud write the answers.
 */
import { readFileSync } from "node:fs";
import { DEFAULT_ACCOUNT_ID, DEFAULT_PRIVATE_KEY, Sandbox } from "near-sandbox";
import { Account, KeyPair, nearToYocto, yoctoToNear, type KeyPairString } from "near-api-js";

const WASM = new URL("../../res/franchise.wasm", import.meta.url);
const TGAS = 1_000_000_000_000n;
const near = (y: string | bigint) => `${Number(yoctoToNear(BigInt(y), 4))} NEAR`;
const step = (n: number, title: string) => console.log(`\n\x1b[1m${n}. ${title}\x1b[0m`);
const note = (msg: string) => console.log(`   ${msg}`);

const sandbox = await Sandbox.start({});
try {
  const contract = `franchise.${DEFAULT_ACCOUNT_ID}`;
  process.env.NEAR_RPC_URL = sandbox.rpcUrl;
  process.env.FRANCHISE_CONTRACT = contract;
  // Imported after the env is set: these modules read it at load time.
  const { view, keyIsActive } = await import("./chain.js");
  const { tick } = await import("./runtime.js");
  const { useNearAi } = await import("./llm.js");

  const root = new Account(DEFAULT_ACCOUNT_ID, sandbox.rpcUrl, DEFAULT_PRIVATE_KEY as KeyPairString);
  const make = async (prefix: string, near: string) => {
    const key = KeyPair.fromRandom("ed25519");
    await root.createSubAccount({ accountOrPrefix: prefix, publicKey: key.getPublicKey(), nearToTransfer: nearToYocto(near as `${number}`) });
    return new Account(`${prefix}.${DEFAULT_ACCOUNT_ID}`, sandbox.rpcUrl, key.toString());
  };

  step(0, `Local NEAR node at ${sandbox.rpcUrl}`);
  const franchise = await make("franchise", "20");
  await franchise.deployContract(readFileSync(WASM));
  await franchise.callFunction({ contractId: contract, methodName: "new", args: { job_timeout_sec: 600 } });
  const [alice, bob, carol] = await Promise.all([make("alice", "20"), make("bob", "20"), make("carol", "20")]);
  note(`franchise contract: ${contract}`);
  note(`alice = creator, bob = buyer, carol = client · answers by ${useNearAi() ? "NEAR AI Cloud" : "mock model (set NEARAI_API_KEY for real ones)"}`);

  step(1, "Alice launches an agent business: mints NFT #1");
  const cost = await view<string>("mint_cost");
  const tokenId = await alice.callFunction<string>({
    contractId: contract,
    methodName: "mint",
    args: { name: "Haiku Bot", skill: "You write a single haiku (5-7-5) about the topic you are given.", price: nearToYocto("0.1").toString() },
    deposit: BigInt(cost),
    gas: 300n * TGAS,
  });
  const vault = `agent-${tokenId}.${contract}`;
  const keysAfterMint = await new Account(vault, sandbox.rpcUrl).getAccessKeyList();
  note(`NFT #${tokenId} minted to alice for ${near(cost)} (unused part refunded)`);
  note(`its own NEAR account: ${vault} · access keys: ${keysAfterMint.keys.length} (nobody holds a full-access key)`);

  step(2, "Alice hires the runtime: registers an operator key");
  const aliceKey = KeyPair.fromRandom("ed25519").toString();
  await alice.callFunction({
    contractId: contract,
    methodName: "set_operator",
    args: { token_id: tokenId, public_key: KeyPair.fromString(aliceKey as KeyPairString).getPublicKey().toString() },
    deposit: 1n,
    gas: 100n * TGAS,
  });
  const [k] = (await new Account(vault, sandbox.rpcUrl).getAccessKeyList()).keys;
  const perm = k.access_key.permission as { FunctionCall?: { method_names: string[]; receiver_id: string } };
  note(`vault key: FunctionCall → ${perm.FunctionCall?.receiver_id} [${perm.FunctionCall?.method_names.join(", ")}]`);

  step(3, "Carol hires Haiku Bot for 0.1 NEAR");
  const cfg = await view<{ job_storage_fee: string }>("get_config");
  const jobId = await carol.callFunction<string>({
    contractId: contract,
    methodName: "hire",
    args: { token_id: tokenId, task: "Owning a piece of an AI that works while you sleep" },
    deposit: nearToYocto("0.1") + BigInt(cfg.job_storage_fee),
    gas: 50n * TGAS,
  });
  note(`job ${jobId} is in escrow`);

  step(4, "The runtime answers and signs complete_job as the vault");
  await tick(tokenId, aliceKey);
  const job = await view<{ result: string; status: string }>("get_job", { job_id: Number(jobId) });
  note(`status: ${job.status}`);
  console.log(job.result.split("\n").map((l) => `   │ ${l}`).join("\n"));
  let agent = await view<{ total_earned: string; royalties_paid: string; owner: string; paused: boolean }>("get_agent", { token_id: tokenId });
  note(`vault earned ${near(agent.total_earned)} · creator royalty ${near(agent.royalties_paid)}`);

  step(5, "Alice sells NFT #1 to Bob");
  await alice.callFunction({ contractId: contract, methodName: "nft_transfer", args: { receiver_id: bob.accountId, token_id: tokenId }, deposit: 1n, gas: 100n * TGAS });
  agent = await view("get_agent", { token_id: tokenId });
  const vaultState = await view<{ owner: string; available: string; epoch: number }>("get_state", {}, vault);
  note(`holder: ${agent.owner} · agent paused: ${agent.paused} · vault epoch: ${vaultState.epoch}`);
  note(`alice's operator key still active? ${await keyIsActive(vault, aliceKey)}`);
  note(`the vault's ${near(vaultState.available)} of withdrawable earnings moved with the NFT`);
  await tick(tokenId, aliceKey); // the runtime notices and refuses to work for the old holder

  step(6, "Bob registers his own operator key; Carol hires again");
  const bobKey = KeyPair.fromRandom("ed25519").toString();
  await bob.callFunction({
    contractId: contract,
    methodName: "set_operator",
    args: { token_id: tokenId, public_key: KeyPair.fromString(bobKey as KeyPairString).getPublicKey().toString() },
    deposit: 1n,
    gas: 100n * TGAS,
  });
  await carol.callFunction({
    contractId: contract,
    methodName: "hire",
    args: { token_id: tokenId, task: "A sale that cannot be cheated" },
    deposit: nearToYocto("0.1") + BigInt(cfg.job_storage_fee),
    gas: 50n * TGAS,
  });
  await tick(tokenId, bobKey);

  step(7, "Alice tries to withdraw, then Bob withdraws everything");
  try {
    await alice.callFunction({ contractId: contract, methodName: "withdraw", args: { token_id: tokenId }, deposit: 1n, gas: 100n * TGAS });
    note("alice withdrew (unexpected!)");
  } catch (e) {
    note(`alice: rejected (${(e as Error).message.match(/only the current holder/)?.[0] ?? "error"})`);
  }
  const sent = await bob.callFunction<string>({ contractId: contract, methodName: "withdraw", args: { token_id: tokenId }, deposit: 1n, gas: 100n * TGAS });
  note(`bob withdrew ${near(sent)} from the agent's account`);

  const history = await view<{ kind: string; actor: string }[]>("get_history", { token_id: tokenId });
  step(8, "NFT #1 history (newest first)");
  for (const h of history) note(`${h.kind.padEnd(12)} ${h.actor}`);
  console.log("\nDone.");

  if (process.argv.includes("--keep")) {
    const extra = [
      { who: carol, name: "Translator", skill: "You translate the given text into French, Spanish and Hindi.", price: "0.05", task: "Your agent works while you sleep." },
      { who: bob, name: "Thread Writer", skill: "You turn a product idea into a punchy 3-post thread for X.", price: "0.2", task: "NFTs that own an AI worker's NEAR account" },
    ];
    const keys: Record<string, string> = { [tokenId]: bobKey };
    for (const e of extra) {
      const id = await e.who.callFunction<string>({ contractId: contract, methodName: "mint", args: { name: e.name, skill: e.skill, price: nearToYocto(e.price as `${number}`).toString() }, deposit: BigInt(cost), gas: 300n * TGAS });
      const key = KeyPair.fromRandom("ed25519").toString();
      keys[id] = key;
      await e.who.callFunction({ contractId: contract, methodName: "set_operator", args: { token_id: id, public_key: KeyPair.fromString(key as KeyPairString).getPublicKey().toString() }, deposit: 1n, gas: 100n * TGAS });
      await alice.callFunction({ contractId: contract, methodName: "hire", args: { token_id: id, task: e.task }, deposit: nearToYocto(e.price as `${number}`) + BigInt(cfg.job_storage_fee), gas: 50n * TGAS });
    }
    console.log(`\nNode kept running. Point the web app at it:\n  VITE_RPC_URL=${sandbox.rpcUrl} VITE_CONTRACT=${contract} npm run dev   (in web/)\nCtrl+C to stop.`);
    for (;;) {
      for (const [id, key] of Object.entries(keys)) await tick(id, key);
      await new Promise((r) => setTimeout(r, 3000));
    }
  }
} finally {
  await sandbox.tearDown();
}
