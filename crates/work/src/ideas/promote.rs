//! The `Promote` executor: a kept idea's branch merged into the fork's `main`, which makes the next champion.
//!
//! The merge is made in a scratch worktree under the data directory (a fast-forward when `main` is behind the branch,
//! else a merge commit carrying the branch's `Bench:` line). Since `main` is usually checked out in the user's
//! checkout, that checkout is fast-forwarded to the result when it has no uncommitted changes, and the promotion is
//! refused, changing nothing, when it has. The new tip becomes champion `v<n+1>` with the Elo of the idea's deciding
//! trial. Pushing is separate and explicit ([`crate::versions::push`]).
//!
//! A **net** idea changes no code, so nothing is merged: Reckless's nets live outside its repository (`networks/` is
//! git-ignored; they are published as releases of a separate repository), and the champion line simply becomes "code
//! X plus net Y" ([`crate::versions::Versions::crown_net`]). Every build of the champion from then on embeds that net,
//! and a later code promotion keeps it. If the code moved since the net was tested, the net goes onto the current
//! champion's code (the log says so); its trials were measured on the older code.

use async_trait::async_trait;
use dameless_core::{
    Error, IdeaId,
    ideas::{IdeaKind, IdeaStatus},
    work::WorkKind,
};
use dameless_engine::git::{CheckedOut, Promote};
use dameless_hub::{Executor, WorkContext};
use serde::{Deserialize, Serialize};

use super::Ideas;
use crate::versions::MAIN;

/// What a `Promote` work item promotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromoteWork {
    pub idea: IdeaId,
}

pub(super) struct PromoteExecutor {
    pub ideas: Ideas,
}

#[async_trait]
impl Executor for PromoteExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::Promote
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let w: PromoteWork = cx.spec()?;
        let ideas = &self.ideas;
        let versions = ideas.versions();
        let hub = versions.hub();
        let idea = ideas.get(w.idea).await?;
        if idea.status != IdeaStatus::Kept {
            return Err(Error::conflict(format!("Idea {} is not kept", idea.id)));
        }
        let version_id = idea.version.ok_or_else(|| Error::conflict("The idea has no version"))?;
        let version =
            hub.store.version(version_id).await?.ok_or_else(|| Error::not_found("The idea's version is gone"))?;
        // Make sure the line knows the current tip before it grows.
        let champion = versions.champion().await?;
        if idea.kind == IdeaKind::Net {
            let net = version.net.ok_or_else(|| Error::conflict("The net idea's version has no net"))?;
            if champion.commit != version.commit {
                cx.log(format!(
                    "The champion's code moved since the net was tested ({} → {}): the net goes onto {}",
                    &version.commit[..8.min(version.commit.len())],
                    &champion.commit[..8.min(champion.commit.len())],
                    champion.name
                ));
            }
            let crowned = versions.crown_net(&champion.commit, net, version.elo).await?;
            cx.log(format!("{} is the champion: {} with net {}", crowned.name, champion.name, net));
            return Ok(serde_json::json!({
                "idea": idea.id,
                "version": crowned.id,
                "name": crowned.name,
                "commit": crowned.commit,
                "net": net,
            }));
        }
        let what = version.branch.clone().unwrap_or_else(|| version.commit.clone());
        let message = format!(
            "Merge {what}: {}\n\n{}\n\nBench: {}\n",
            idea.title,
            idea.verdict.clone().unwrap_or_default(),
            version.bench.map_or_else(|| "?".to_owned(), |b| b.to_string())
        );
        cx.log(format!("Promoting {what} into {MAIN}"));
        let promoted = versions
            .repo()
            .promote(&Promote {
                branch: &version.commit,
                into: MAIN,
                message: &message,
                worktree: &hub.config.promote_worktree(),
                checked_out: CheckedOut::FastForwardIfClean,
            })
            .await
            .map_err(|e| Error::conflict(format!("{e:#}")))?;
        cx.log(format!(
            "{MAIN} {} → {}{}",
            &promoted.previous[..8.min(promoted.previous.len())],
            &promoted.commit[..8.min(promoted.commit.len())],
            if promoted.fast_forward { " (fast-forward)" } else { " (merge commit)" }
        ));
        let bench = if promoted.fast_forward { version.bench } else { dameless_engine::git::bench_line(&message) };
        let champion = versions.crown(&promoted.commit, bench, version.elo).await?;
        cx.log(format!("{} is the champion", champion.name));
        Ok(serde_json::json!({
            "idea": idea.id,
            "version": champion.id,
            "name": champion.name,
            "commit": promoted.commit,
            "fast_forward": promoted.fast_forward,
        }))
    }
}
