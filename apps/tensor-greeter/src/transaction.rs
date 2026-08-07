use crate::{
    AuthAttemptId, AuthMessageType, AuthPrompt, AuthPromptKind, ErrorType, GreetdClient,
    GreetdClientError, GreeterModel, GreeterModelError, Response,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthUpdate {
    Prompt {
        attempt: AuthAttemptId,
        prompt: AuthPrompt,
    },
    Authenticated {
        attempt: AuthAttemptId,
    },
    Failed {
        attempt: AuthAttemptId,
        message: String,
    },
}

pub struct GreeterTransaction {
    model: GreeterModel,
    client: GreetdClient,
}

impl GreeterTransaction {
    pub fn new(model: GreeterModel, client: GreetdClient) -> Self {
        Self { model, client }
    }

    pub fn model(&self) -> &GreeterModel {
        &self.model
    }

    pub fn model_mut(&mut self) -> &mut GreeterModel {
        &mut self.model
    }

    pub async fn begin_authentication(&mut self) -> Result<AuthUpdate, GreeterTransactionError> {
        let start = self.model.begin_authentication()?;
        let response = self.client.create_session(&start.username).await?;
        apply_response(&mut self.model, start.attempt, response)
    }

    pub async fn respond(
        &mut self,
        attempt: AuthAttemptId,
        response: Option<&str>,
    ) -> Result<AuthUpdate, GreeterTransactionError> {
        self.model.response_sent(attempt, response.is_some())?;
        let response = self.client.post_auth_message_response(response).await?;
        apply_response(&mut self.model, attempt, response)
    }

    pub async fn start_session(
        &mut self,
        attempt: AuthAttemptId,
    ) -> Result<(), GreeterTransactionError> {
        let session = self.model.start_session(attempt)?;
        match self
            .client
            .start_session(&session.command, &session.environment)
            .await?
        {
            Response::Success => {
                self.model.session_started(attempt)?;
                Ok(())
            }
            Response::Error { description, .. } => {
                Err(GreeterTransactionError::Server(description))
            }
            Response::AuthMessage { .. } => Err(GreeterTransactionError::UnexpectedResponse(
                "auth message while starting session",
            )),
        }
    }

    pub async fn cancel(&mut self) -> Result<(), GreeterTransactionError> {
        match self.client.cancel_session().await? {
            Response::Success => {
                self.model.cancel();
                Ok(())
            }
            Response::Error { description, .. } => {
                Err(GreeterTransactionError::Server(description))
            }
            Response::AuthMessage { .. } => Err(GreeterTransactionError::UnexpectedResponse(
                "auth message while cancelling session",
            )),
        }
    }
}

fn apply_response(
    model: &mut GreeterModel,
    attempt: AuthAttemptId,
    response: Response,
) -> Result<AuthUpdate, GreeterTransactionError> {
    match response {
        Response::Success => {
            model.authentication_succeeded(attempt)?;
            Ok(AuthUpdate::Authenticated { attempt })
        }
        Response::AuthMessage {
            auth_message_type,
            auth_message,
        } => {
            let prompt = AuthPrompt {
                kind: match auth_message_type {
                    AuthMessageType::Visible => AuthPromptKind::Visible,
                    AuthMessageType::Secret => AuthPromptKind::Secret,
                    AuthMessageType::Info => AuthPromptKind::Info,
                    AuthMessageType::Error => AuthPromptKind::Error,
                },
                message: auth_message,
            };
            model.receive_prompt(attempt, prompt.clone())?;
            Ok(AuthUpdate::Prompt { attempt, prompt })
        }
        Response::Error {
            error_type,
            description,
        } => {
            let message = match error_type {
                ErrorType::AuthError => description,
                ErrorType::Error => format!("greetd: {description}"),
            };
            model.authentication_failed(attempt, message.clone())?;
            Ok(AuthUpdate::Failed { attempt, message })
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GreeterTransactionError {
    #[error(transparent)]
    Model(#[from] GreeterModelError),
    #[error(transparent)]
    Client(#[from] GreetdClientError),
    #[error("greetd rejected the session transaction: {0}")]
    Server(String),
    #[error("unexpected greetd response: {0}")]
    UnexpectedResponse(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GreeterConfig, UserAccount};

    fn model() -> GreeterModel {
        let config = GreeterConfig::default();
        GreeterModel::new(
            vec![UserAccount::new("tensor", "Tensor User").unwrap()],
            config.sessions,
            config.max_auth_message_bytes,
        )
        .unwrap()
    }

    #[test]
    fn greetd_responses_advance_prompt_success_and_failure_states() {
        let mut model = model();
        let start = model.begin_authentication().unwrap();
        let prompt = apply_response(
            &mut model,
            start.attempt,
            Response::AuthMessage {
                auth_message_type: AuthMessageType::Secret,
                auth_message: "Password:".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            prompt,
            AuthUpdate::Prompt {
                prompt: AuthPrompt {
                    kind: AuthPromptKind::Secret,
                    ..
                },
                ..
            }
        ));

        model.response_sent(start.attempt, true).unwrap();
        assert!(matches!(
            apply_response(&mut model, start.attempt, Response::Success).unwrap(),
            AuthUpdate::Authenticated { .. }
        ));

        model.cancel();
        let retry = model.begin_authentication().unwrap();
        assert!(matches!(
            apply_response(
                &mut model,
                retry.attempt,
                Response::Error {
                    error_type: ErrorType::AuthError,
                    description: "invalid password".into(),
                },
            )
            .unwrap(),
            AuthUpdate::Failed { .. }
        ));
    }
}
