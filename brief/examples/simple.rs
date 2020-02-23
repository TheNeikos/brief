extern crate brief;
use brief::tg::{CanAnswerCallbackQuery, CanReplySendMessage, CanSendMessage};

/// This is Hello World bot. All it does is tell you Hello World
/// and gives you a keyboard to use and write Hello World yourself.
#[brief::bot]
#[derive(Default)]
struct HelloWorldBot {
    #[brief::command = "help"] // Default is the name of the field, but we change it as a showcase
    help_cmd: HelloWorldCommand,
    #[brief::action] // Default is the name of the field
    say_it: SayHelloWorldAction,
}

#[derive(Default)]
struct HelloWorldCommand;

#[brief::async_trait]
impl brief::BotCommand for HelloWorldCommand {
    async fn handle(
        &self,
        ctx: &brief::Context<'_>,
        msg: &brief::tg::Message,
        args: Option<&str>,
        text: &str,
    ) -> Result<(), brief::BriefError> {
        let keyboard: brief::tg::InlineKeyboardMarkup = vec![vec![
            brief::tg::InlineKeyboardButton::callback(
                "Cats",
                SayHelloWorldAction::callback_data("Cats"),
            ),
            brief::tg::InlineKeyboardButton::callback(
                "Rabbits",
                SayHelloWorldAction::callback_data("Rabbits"),
            ),
        ]]
        .into();
        let reply = msg
            .text_reply(
                "Hello there!\n\n\
                I am a testing bot for @TheNeikos. And am meant to showcase a simple bot \
                with whom you can interact. Have fun and be sure to check out the library!",
            )
            .reply_markup(keyboard);
        ctx.send_request(reply).await?;
        Ok(())
    }
}

#[derive(Default)]
struct SayHelloWorldAction;

#[brief::async_trait]
impl brief::BotAction for SayHelloWorldAction {
    async fn handle(
        &self,
        ctx: &brief::Context<'_>,
        callback: &brief::tg::CallbackQuery,
        args: Option<String>,
    ) -> Result<(), brief::BriefError> {
        ctx.send_request(
            callback
                .answer(format!("{} are the best!", args.unwrap()))
                .show_alert(),
        )
        .await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    brief::start(
        HelloWorldBot::default(),
        "1022844841:AAGhC9u0KQdzzBJAtNG626vum7amOVZaHbU",
    )
    .await?;
    Ok(())
}
