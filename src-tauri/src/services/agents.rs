//! Grok CLI and Claude Code, the authoring agents a project can pick instead of
//! the bundled Copilot CLI. Copilot stays the default. These two run only when
//! they are installed on this computer.

use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::commands::screenshot;
use crate::services::emit::emit_chat_event;
use crate::services::exec::CancelToken;
use crate::services::project_mutation::MutationGuard;
use crate::state::AppState;
use crate::types::{AgentOption, ChatEvent, ChatTurnResult, CopilotModel, StudioProject};
use tauri::AppHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentKind {
  Copilot,
  Grok,
  Claude,
}

impl AgentKind {
  pub fn parse(value: Option<&str>) -> Self {
    match value.unwrap_or("copilot").trim().to_ascii_lowercase().as_str() {
      "grok" => Self::Grok,
      "claude" => Self::Claude,
      _ => Self::Copilot,
    }
  }

  pub fn is_external(self) -> bool {
    !matches!(self, Self::Copilot)
  }

  /// What to store on the project. Copilot is stored as absent.
  pub fn stored(self) -> Option<String> {
    match self {
      Self::Copilot => None,
      Self::Grok => Some("grok".into()),
      Self::Claude => Some("claude".into()),
    }
  }

  fn program(self) -> &'static str {
    match self {
      Self::Grok => "grok",
      Self::Claude => "claude",
      Self::Copilot => "copilot",
    }
  }

  fn label(self) -> &'static str {
    match self {
      Self::Grok => "Grok CLI",
      Self::Claude => "Claude Code",
      Self::Copilot => "GitHub Copilot",
    }
  }
}

pub fn accepts_model(kind: AgentKind, model: &str) -> bool {
  if model.trim().is_empty() {
    return true;
  }
  match kind {
    AgentKind::Copilot => true,
    other => models(other.program()).iter().any(|item| item.id == model),
  }
}

pub fn options() -> Vec<AgentOption> {
  [AgentKind::Copilot, AgentKind::Grok, AgentKind::Claude]
    .into_iter()
    .map(|kind| {
      let installed = kind == AgentKind::Copilot || which::which(kind.program()).is_ok();
      AgentOption {
        id: kind.stored().unwrap_or_else(|| "copilot".into()),
        name: kind.label().into(),
        installed,
        hint: (!installed).then(|| format!("Install {} and sign in, then pick it again.", kind.label())),
      }
    })
    .collect()
}

pub fn models(agent: &str) -> Vec<CopilotModel> {
  let ids: &[&str] = match AgentKind::parse(Some(agent)) {
    AgentKind::Grok => &["grok-3", "grok-3-mini"],
    AgentKind::Claude => &["claude-sonnet-4.5", "claude-opus-4.1", "claude-haiku-4.5"],
    AgentKind::Copilot => &[],
  };
  ids.iter()
    .map(|id| CopilotModel {
      id: (*id).into(),
      name: (*id).into(),
      supported_reasoning_efforts: Vec::new(),
      default_reasoning_effort: None,
      category: None,
      price_category: None,
      vision: true,
      vendor: None,
      context_tiers: Vec::new(),
      byok: None,
    })
    .collect()
}

#[allow(clippy::too_many_arguments)]
pub async fn run_external_turn(
  app: AppHandle,
  state: &AppState,
  project: StudioProject,
  project_id: String,
  turn_id: String,
  text: String,
  attachments: Vec<String>,
  mode: Option<String>,
  token: CancelToken,
  _mutation: MutationGuard,
) -> Result<ChatTurnResult, String> {
  let kind = AgentKind::parse(project.agent.as_deref());
  let finish = |ok: bool, error: Option<String>| {
    state.end_chat(&project_id);
    screenshot::cleanup(&attachments);
    ChatTurnResult { ok, error, files_modified: vec![], ran_deploy: false }
  };
  if !kind.is_external() {
    return Ok(finish(false, Some("That isn't an external agent.".into())));
  }
  if which::which(kind.program()).is_err() {
    let message = format!("{} isn't installed on this computer.", kind.label());
    emit_chat_event(&app, &project_id, &turn_id, ChatEvent::Error { text: message.clone() });
    return Ok(finish(false, Some(message)));
  }
  if mode.as_deref() == Some("plan") {
    let message = format!("{} answers in the project terminal for Plan mode. Switch to Agent, or use Copilot.", kind.label());
    emit_chat_event(&app, &project_id, &turn_id, ChatEvent::Error { text: message.clone() });
    return Ok(finish(false, Some(message)));
  }

  emit_chat_event(&app, &project_id, &turn_id, ChatEvent::Notice { text: format!("Running {}…", kind.label()) });
  let mut child = Command::new(kind.program());
  child
    .arg("-p")
    .current_dir(Path::new(&project.path))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  let mut child = match child.spawn() {
    Ok(child) => child,
    Err(error) => {
      let message = format!("Couldn't start {}: {error}", kind.label());
      emit_chat_event(&app, &project_id, &turn_id, ChatEvent::Error { text: message.clone() });
      return Ok(finish(false, Some(message)));
    }
  };
  if let Some(mut stdin) = child.stdin.take() {
    let _ = stdin.write_all(text.as_bytes()).await;
    let _ = stdin.shutdown().await;
  }
  let stdout = child.stdout.take();
  let stderr = child.stderr.take();
  let app_err = app.clone();
  let (project_err, turn_err) = (project_id.clone(), turn_id.clone());
  let errors = tokio::spawn(async move {
    if let Some(stderr) = stderr {
      let mut lines = BufReader::new(stderr).lines();
      while let Ok(Some(line)) = lines.next_line().await {
        if !line.trim().is_empty() {
          emit_chat_event(&app_err, &project_err, &turn_err, ChatEvent::Notice { text: line });
        }
      }
    }
  });
  if let Some(stdout) = stdout {
    let mut lines = BufReader::new(stdout).lines();
    loop {
      if token.is_cancelled() {
        let _ = child.kill().await;
        break;
      }
      match lines.next_line().await {
        Ok(Some(line)) => emit_chat_event(&app, &project_id, &turn_id, ChatEvent::Delta { text: format!("{line}\n") }),
        Ok(None) => break,
        Err(error) => {
          emit_chat_event(&app, &project_id, &turn_id, ChatEvent::Error { text: error.to_string() });
          break;
        }
      }
    }
  }
  let status = child.wait().await.ok();
  let _ = errors.await;
  let ok = status.as_ref().is_some_and(|status| status.success()) && !token.is_cancelled();
  emit_chat_event(
    &app,
    &project_id,
    &turn_id,
    ChatEvent::Result { ok, files_modified: vec![], ran_deploy: false },
  );
  let error = if token.is_cancelled() {
    Some("Stopped.".into())
  } else if ok {
    None
  } else {
    Some(format!("{} exited without finishing.", kind.label()))
  };
  Ok(finish(ok, error))
}
