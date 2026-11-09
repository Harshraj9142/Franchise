import { useEffect, useState } from "react";
import { CONTRACT, NETWORK, REPO_URL } from "./config";
import { AccountLink } from "./ui";
import { signIn, signOut, useAccount } from "./wallet";
import { AgentPage } from "./pages/AgentPage";
import { Market } from "./pages/Market";

function useHash() {
  const [hash, setHash] = useState(location.hash);
  useEffect(() => {
    const on = () => {
      setHash(location.hash);
      if (!location.hash.startsWith("#launch")) window.scrollTo(0, 0);
    };
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return hash;
}

export function App() {
  const hash = useHash();
  const account = useAccount();
  const agentMatch = hash.match(/^#\/agent\/(\d+)/);

  if (!CONTRACT) {
    return (
      <main className="page">
        <p className="error">VITE_CONTRACT is not set. Copy web/.env.example to web/.env and set the franchise contract account.</p>
      </main>
    );
  }

  return (
    <>
      <nav className="nav">
        <a className="brand" href="#/">
          <span className="logo">F</span> Franchise
        </a>
        <span className="pill net">{NETWORK}</span>
        <span className="spacer" />
        {account ? (
          <span className="row">
            <AccountLink id={account} />
            <button className="btn ghost small" onClick={() => signOut()}>
              Sign out
            </button>
          </span>
        ) : (
          <button className="btn primary" onClick={() => signIn()}>
            Connect wallet
          </button>
        )}
      </nav>
      <main className="page">{agentMatch ? <AgentPage tokenId={agentMatch[1]} /> : <Market />}</main>
      <footer className="foot">
        <span>
          Contract <AccountLink id={CONTRACT} max={40} /> · built at NEAR HACKATHON
        </span>
        {REPO_URL && (
          <a href={REPO_URL} target="_blank" rel="noreferrer">
            source code
          </a>
        )}
      </footer>
    </>
  );
}
