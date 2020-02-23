pub use async_trait::async_trait;
pub use brief_derive::bot;
pub use telegram_bot_async_raw as tg;

pub struct Nothing;

impl tg::Request for Nothing {
    type Type = NothingRequestType;
    type Response = tg::JsonIdResponse<()>;

    fn serialize(&self) -> Result<tg::HttpRequest, tg::Error> {
        Err(tg::Error::EmptyBody)
    }
}

pub struct NothingRequestType;

impl tg::RequestType for NothingRequestType {
    type Options = ();
    type Request = ();

    fn serialize(
        _options: Self::Options,
        _request: &Self::Request,
    ) -> Result<tg::HttpRequest, tg::Error> {
        Err(tg::Error::EmptyBody)
    }
}

#[async_trait]
pub trait BotCommand: Send + Sync {
    async fn handle(
        &self,
        ctx: &Context<'_>,
        msg: &tg::Message,
        args: Option<&str>,
        text: &str,
    ) -> Result<(), BriefError>;
}

#[async_trait]
pub trait BotAction: Send + Sync {
    async fn handle(
        &self,
        ctx: &Context<'_>,
        callback: &tg::CallbackQuery,
        args: Option<String>,
    ) -> Result<(), BriefError>;
}

#[async_trait]
pub trait TelegramBot: Sync + Send {
    async fn handle_message(
        &self,
        _ctx: &Context<'_>,
        _msg: &tg::Message,
    ) -> Result<Propagate, BriefError> {
        Ok(Propagate::Continue)
    }

    async fn handle_command(
        &self,
        _ctx: &Context<'_>,
        _cmd: &str,
        _args: Option<&str>,
        _text: &str,
        _message: &tg::Message,
    ) -> Result<Propagate, BriefError> {
        Ok(Propagate::Continue)
    }

    async fn handle_callback(
        &self,
        _ctx: &Context<'_>,
        _callback: &tg::CallbackQuery,
    ) -> Result<(), BriefError> {
        Ok(())
    }
}

pub struct Context<'a> {
    token: &'a str,
}

impl<'a> Context<'a> {
    pub async fn send_request<R: tg::Request>(
        &self,
        req: R,
    ) -> Result<<R::Response as tg::ResponseType>::Type, BriefError> {
        use reqwest::Client;
        let client = Client::new();

        let req = req.serialize().unwrap();

        let method = reqwest::Method::POST;

        let request = client.request(method, &req.url.url(None, self.token));

        let body = match req.body {
            tg::Body::Empty => vec![],
            tg::Body::Json(v) => v,
        };

        let request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);

        let res = request.send().await?;

        let status = res.status();
        let resp_body = res.bytes().await?;

        if !status.is_success() {
            return Err(BriefError::ResponseError(status, resp_body));
        }

        let body = resp_body.to_vec();

        let data =
            <R::Response as tg::ResponseType>::deserialize(tg::HttpResponse { body: Some(body) })
                .unwrap();

        Ok(data)
    }
}

pub async fn start<T: TelegramBot>(bot: T, token: &str) -> Result<(), BriefError> {
    let ctx = Context { token };
    let my_info = ctx.send_request(tg::GetMe).await?;

    let botname = my_info.username.unwrap(); // It has to have a username

    let mut last_message = 0;

    'outer: loop {
        let mut updates = ctx
            .send_request(tg::GetUpdates::new().offset(last_message).timeout(10))
            .await?;

        updates.sort_unstable_by(|l, r| l.id.cmp(&r.id));

        for update in updates {
            last_message = update.id + 1;
            match update.kind {
                tg::UpdateKind::Message(msg) => {
                    if let Propagate::Stop = bot.handle_message(&ctx, &msg).await? {
                        continue 'outer;
                    }

                    if let tg::MessageKind::Text { data, entities } = &msg.kind {
                        for command in entities
                            .iter()
                            .filter(|entity| entity.kind == tg::MessageEntityKind::BotCommand)
                        {
                            let cmd_end = (command.offset + command.length) as usize;
                            let cmd = &data[(command.offset as usize)..cmd_end];
                            let cmd_name = cmd.split('@').collect::<Vec<_>>();

                            let mut can_be_for_this_bot = true;

                            if let Some(name) = cmd_name.get(1) {
                                if name != &botname {
                                    can_be_for_this_bot = false;
                                }
                            }

                            if can_be_for_this_bot {
                                let args = if command.offset == 0 {
                                    if data.len() > cmd_end {
                                        Some(&data[cmd_end..])
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                bot.handle_command(&ctx, &cmd_name[0], args, &data, &msg)
                                    .await?;
                            }
                        }
                    }
                }
                tg::UpdateKind::CallbackQuery(query) => {
                    bot.handle_callback(&ctx, &query).await?;
                }
                _ => (),
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum BriefError {
    #[error("An error originating from the telegram API: {0}")]
    Telegram(u32),
    #[error("A reqwest error occured: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("A response was incorrect: {0}")]
    ResponseError(hyper::StatusCode, hyper::body::Bytes),
}

/// Signals wether a given message should be exclusively handled by that handler
pub enum Propagate {
    Continue,
    Stop,
}
