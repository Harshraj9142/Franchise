//! Read-only methods used by the web app and the agent runtime.
use crate::*;

#[near(serializers = [json])]
#[derive(Clone)]
pub struct AgentView {
    pub token_id: TokenId,
    pub name: String,
    pub skill: String,
    pub price: U128,
    pub creator: AccountId,
    pub owner: AccountId,
    pub vault: AccountId,
    pub epoch: u64,
    pub operator_set: bool,
    pub paused: bool,
    pub jobs_completed: u64,
    pub open_jobs: u32,
    pub total_earned: U128,
    pub royalties_paid: U128,
    pub total_withdrawn: U128,
    pub created_at: U64,
}

#[near(serializers = [json])]
pub struct JobView {
    pub id: u64,
    pub token_id: TokenId,
    pub client: AccountId,
    pub task: String,
    pub price: U128,
    pub status: JobStatus,
    pub result: Option<String>,
    pub created_at: U64,
    pub completed_at: Option<U64>,
    pub holder_at_completion: Option<AccountId>,
}

#[near(serializers = [json])]
pub struct HistoryView {
    pub id: u64,
    pub kind: HistoryKind,
    pub token_id: TokenId,
    pub actor: AccountId,
    pub amount: Option<U128>,
    pub job_id: Option<u64>,
    pub epoch: u64,
    pub timestamp: U64,
}

#[near(serializers = [json])]
pub struct ConfigView {
    pub mint_cost: U128,
    pub job_storage_fee: U128,
    pub min_price: U128,
    pub creator_royalty_bps: u16,
    pub job_timeout_sec: u64,
    pub uses_global_vault: bool,
    pub embedded_vault_bytes: u32,
    pub total_agents: u32,
    pub total_jobs: u32,
}

impl Contract {
    fn agent_view(&self, token_id: &TokenId, a: &Agent) -> AgentView {
        let open_jobs = a
            .job_ids
            .iter()
            .filter(|id| self.jobs.get(**id as u32).map(|j| j.status == JobStatus::Open).unwrap_or(false))
            .count() as u32;
        AgentView {
            token_id: token_id.clone(),
            name: a.name.clone(),
            skill: a.skill.clone(),
            price: U128(a.price.as_yoctonear()),
            creator: a.creator.clone(),
            owner: self.tokens.owner_by_id.get(token_id).unwrap_or_else(|| a.creator.clone()),
            vault: a.vault.clone(),
            epoch: a.epoch,
            operator_set: a.operator_set,
            paused: a.paused,
            jobs_completed: a.jobs_completed,
            open_jobs,
            total_earned: U128(a.total_earned.as_yoctonear()),
            royalties_paid: U128(a.royalties_paid.as_yoctonear()),
            total_withdrawn: U128(a.total_withdrawn.as_yoctonear()),
            created_at: U64(a.created_at),
        }
    }

    fn job_view(j: &Job) -> JobView {
        JobView {
            id: j.id,
            token_id: j.token_id.clone(),
            client: j.client.clone(),
            task: j.task.clone(),
            price: U128(j.price.as_yoctonear()),
            status: j.status.clone(),
            result: j.result.clone(),
            created_at: U64(j.created_at),
            completed_at: j.completed_at.map(U64),
            holder_at_completion: j.holder_at_completion.clone(),
        }
    }

    fn history_view(&self, id: u64) -> Option<HistoryView> {
        self.history.get(id as u32).map(|h| HistoryView {
            id,
            kind: h.kind.clone(),
            token_id: h.token_id.clone(),
            actor: h.actor.clone(),
            amount: h.amount.map(|a| U128(a.as_yoctonear())),
            job_id: h.job_id,
            epoch: h.epoch,
            timestamp: U64(h.timestamp),
        })
    }
}

#[near]
impl Contract {
    pub fn get_config(&self) -> ConfigView {
        ConfigView {
            mint_cost: U128(self.mint_cost_internal().as_yoctonear()),
            job_storage_fee: U128(JOB_STORAGE_FEE.as_yoctonear()),
            min_price: U128(MIN_PRICE.as_yoctonear()),
            creator_royalty_bps: self.creator_royalty_bps,
            job_timeout_sec: self.job_timeout_ns / 1_000_000_000,
            uses_global_vault: self.vault_code_hash.is_some(),
            embedded_vault_bytes: VAULT_WASM.len() as u32,
            total_agents: self.agents.len(),
            total_jobs: self.jobs.len(),
        }
    }

    pub fn mint_cost(&self) -> U128 {
        U128(self.mint_cost_internal().as_yoctonear())
    }

    pub fn get_agent(&self, token_id: TokenId) -> Option<AgentView> {
        self.agents.get(&token_id).map(|a| self.agent_view(&token_id, a))
    }

    pub fn list_agents(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<AgentView> {
        self.agents
            .iter()
            .skip(from_index.unwrap_or(0) as usize)
            .take(limit.unwrap_or(50) as usize)
            .map(|(id, a)| self.agent_view(id, a))
            .collect()
    }

    /// Newest first.
    pub fn get_jobs(&self, token_id: TokenId, from_index: Option<u32>, limit: Option<u32>) -> Vec<JobView> {
        let Some(a) = self.agents.get(&token_id) else { return vec![] };
        a.job_ids
            .iter()
            .rev()
            .skip(from_index.unwrap_or(0) as usize)
            .take(limit.unwrap_or(20) as usize)
            .filter_map(|id| self.jobs.get(*id as u32).map(Self::job_view))
            .collect()
    }

    /// Oldest first, so the runtime works through them in order.
    pub fn get_open_jobs(&self, token_id: TokenId) -> Vec<JobView> {
        let Some(a) = self.agents.get(&token_id) else { return vec![] };
        a.job_ids
            .iter()
            .filter_map(|id| self.jobs.get(*id as u32))
            .filter(|j| j.status == JobStatus::Open)
            .map(Self::job_view)
            .collect()
    }

    pub fn get_job(&self, job_id: u64) -> Option<JobView> {
        self.jobs.get(job_id as u32).map(Self::job_view)
    }

    /// Newest first, across all agents. For the activity feed.
    pub fn get_recent_jobs(&self, limit: Option<u32>) -> Vec<JobView> {
        let n = self.jobs.len();
        let take = limit.unwrap_or(20).min(n);
        (0..take).filter_map(|i| self.jobs.get(n - 1 - i).map(Self::job_view)).collect()
    }

    /// Newest first: everything this NFT earned, paid out, and who held it.
    pub fn get_history(&self, token_id: TokenId, from_index: Option<u32>, limit: Option<u32>) -> Vec<HistoryView> {
        let Some(a) = self.agents.get(&token_id) else { return vec![] };
        a.history_ids
            .iter()
            .rev()
            .skip(from_index.unwrap_or(0) as usize)
            .take(limit.unwrap_or(50) as usize)
            .filter_map(|id| self.history_view(*id))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Combined views: one RPC call per screen, so public RPC rate limits aren't hit.

#[near(serializers = [json])]
pub struct MarketView {
    pub config: ConfigView,
    pub agents: Vec<AgentView>,
    pub recent_jobs: Vec<JobView>,
}

#[near(serializers = [json])]
pub struct AgentPageView {
    pub config: ConfigView,
    pub agent: AgentView,
    pub jobs: Vec<JobView>,
    pub history: Vec<HistoryView>,
}

#[near(serializers = [json])]
pub struct WorkItem {
    pub job: JobView,
    pub agent: AgentView,
}

#[near]
impl Contract {
    /// Everything the market page shows, in one call.
    pub fn get_market(&self, limit: Option<u32>, recent: Option<u32>) -> MarketView {
        MarketView {
            config: self.get_config(),
            agents: self.list_agents(None, Some(limit.unwrap_or(100))),
            recent_jobs: self.get_recent_jobs(Some(recent.unwrap_or(6))),
        }
    }

    /// Everything an agent's page shows (except its vault's own state), in one call.
    pub fn get_agent_page(&self, token_id: TokenId, jobs_limit: Option<u32>, history_limit: Option<u32>) -> Option<AgentPageView> {
        let agent = self.get_agent(token_id.clone())?;
        Some(AgentPageView {
            config: self.get_config(),
            agent,
            jobs: self.get_jobs(token_id.clone(), None, Some(jobs_limit.unwrap_or(20))),
            history: self.get_history(token_id, None, Some(history_limit.unwrap_or(40))),
        })
    }

    /// Open jobs across all unpaused agents, oldest first, each with its agent.
    /// Lets one runtime serve every agent with a single call per poll.
    pub fn get_open_work(&self, limit: Option<u32>) -> Vec<WorkItem> {
        let limit = limit.unwrap_or(50) as usize;
        let mut items: Vec<WorkItem> = vec![];
        for (token_id, a) in self.agents.iter() {
            if a.paused {
                continue;
            }
            let open: Vec<JobView> = a
                .job_ids
                .iter()
                .filter_map(|id| self.jobs.get(*id as u32))
                .filter(|j| j.status == JobStatus::Open)
                .map(Self::job_view)
                .collect();
            if open.is_empty() {
                continue;
            }
            let agent = self.agent_view(token_id, a);
            for job in open {
                items.push(WorkItem { job, agent: agent.clone() });
            }
        }
        items.sort_by_key(|w| w.job.id);
        items.truncate(limit);
        items
    }
}

