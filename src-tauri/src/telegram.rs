//! Telegram: talk to your bots from your phone. You make a Telegram bot of your own with
//! @BotFather and paste its token into AgentDeck (kept in the keychain); AgentDeck asks Telegram
//! for new messages itself (long polling), so your Mac needs no address on the internet and no
//! server sits in between. Only chats you pair (a code shown in AgentDeck, sent as `/pair 123456`)
//! are answered, and only private ones.
//!
//! In a paired chat a message is a task for a bot; the bot's reply comes back there, and when
//! a bot needs you (a risky request, a question, a plan) the request comes with buttons.

use crate::agents::Agents;
use crate::bots::Bots;
use crate::claude::{Decision, Outbound, Sink};
use crate::db::{now_ms, Bot, BotTask, Db, StoredEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Keychain name of the bot token.
const TOKEN: &str = "telegram-bot-token";
const CHATS: &str = "telegram.chats";
const OFFSET: &str = "telegram.offset";
const NOTIFY: &str = "telegram.notify";
/// Telegram's limit is 4096 characters a message.
const CHUNK: usize = 3800;
const PAIR_MINUTES: i64 = 10;
const PAIR_TRIES: u32 = 5;

/* ---------------------------------------------------------------- the API */

/// The few Bot API calls AgentDeck makes (a fake one in tests).
pub trait Api: Send + Sync {
    /// The bot's @username.
    fn get_me(&self) -> Result<String, String>;
    fn get_updates(&self, offset: i64, wait_secs: u64) -> Result<Vec<Value>, String>;
    /// The message id.
    fn send(
        &self,
        chat: i64,
        text: &str,
        keyboard: Option<Value>,
        reply_to: Option<i64>,
    ) -> Result<i64, String>;
    fn edit(
        &self,
        chat: i64,
        message: i64,
        text: &str,
        keyboard: Option<Value>,
    ) -> Result<(), String>;
    fn answer_callback(&self, id: &str, text: &str) -> Result<(), String>;
}

pub struct Http {
    token: String,
    base: String,
    client: reqwest::blocking::Client,
}

impl Http {
    pub fn new(token: &str) -> Result<Self, String> {
        Ok(Self {
            token: token.trim().to_string(),
            base: "https://api.telegram.org".into(),
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .map_err(|e| e.without_url().to_string())?,
        })
    }

    fn call(&self, method: &str, body: Value) -> Result<Value, String> {
        let url = format!("{}/bot{}/{method}", self.base, self.token);
        // Errors never carry the URL: it holds the token.
        let res = self
            .client
            .post(url)
            .json(&body)
            .send()
            .map_err(|e| format!("Couldn't reach Telegram: {}", e.without_url()))?;
        let v: Value = res
            .json()
            .map_err(|e| format!("Telegram sent something unexpected: {}", e.without_url()))?;
        if v["ok"].as_bool() == Some(true) {
            Ok(v["result"].clone())
        } else {
            Err(v["description"]
                .as_str()
                .unwrap_or("Telegram refused the request")
                .replace(&self.token, "…"))
        }
    }
}

impl Api for Http {
    fn get_me(&self) -> Result<String, String> {
        let me = self.call("getMe", json!({}))?;
        Ok(me["username"].as_str().unwrap_or("").to_string())
    }
    fn get_updates(&self, offset: i64, wait_secs: u64) -> Result<Vec<Value>, String> {
        let v = self.call(
            "getUpdates",
            json!({ "offset": offset, "timeout": wait_secs, "allowed_updates": ["message", "callback_query"] }),
        )?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }
    fn send(
        &self,
        chat: i64,
        text: &str,
        keyboard: Option<Value>,
        reply_to: Option<i64>,
    ) -> Result<i64, String> {
        let mut body = json!({ "chat_id": chat, "text": text, "link_preview_options": { "is_disabled": true } });
        if let Some(k) = keyboard {
            body["reply_markup"] = json!({ "inline_keyboard": k });
        }
        if let Some(r) = reply_to {
            body["reply_parameters"] =
                json!({ "message_id": r, "allow_sending_without_reply": true });
        }
        let m = self.call("sendMessage", body)?;
        Ok(m["message_id"].as_i64().unwrap_or(0))
    }
    fn edit(
        &self,
        chat: i64,
        message: i64,
        text: &str,
        keyboard: Option<Value>,
    ) -> Result<(), String> {
        let mut body = json!({ "chat_id": chat, "message_id": message, "text": text, "link_preview_options": { "is_disabled": true } });
        body["reply_markup"] = json!({ "inline_keyboard": keyboard.unwrap_or(json!([])) });
        self.call("editMessageText", body).map(|_| ())
    }
    fn answer_callback(&self, id: &str, text: &str) -> Result<(), String> {
        self.call(
            "answerCallbackQuery",
            json!({ "callback_query_id": id, "text": text }),
        )
        .map(|_| ())
    }
}

/* ----------------------------------------------------------- what it shows */

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: i64,
    pub name: String,
    pub paired_at: i64,
}

/// What the Settings page shows.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// A token is saved.
    pub connected: bool,
    /// The bot's @username, once Telegram said who it is.
    pub username: Option<String>,
    /// The last thing that went wrong talking to Telegram (cleared when it works again).
    pub error: Option<String>,
    pub chats: Vec<Chat>,
    /// The code to send as `/pair <code>`, while pairing.
    pub pairing_code: Option<String>,
    pub pairing_until: Option<i64>,
    /// Which bot tasks report back: "telegram" (only tasks sent from Telegram), "schedules"
    /// (also scheduled ones) or "all".
    pub notify: String,
}

struct Pairing {
    code: String,
    until: i64,
    tries: u32,
}

/// A request waiting for you that was sent to Telegram.
struct Prompt {
    session_id: String,
    request_id: String,
    /// The bot task it came up in: when that ends, the request is over.
    task_id: Option<String>,
    bot_name: String,
    /// The message text, kept to edit it when it's answered.
    text: String,
    /// (chat, message) it was sent as.
    sent: Vec<(i64, i64)>,
    kind: PromptKind,
}

enum PromptKind {
    Approval,
    Plan,
    Question {
        questions: Vec<Question>,
        /// Question text → chosen labels (or your own words).
        answers: HashMap<String, Vec<String>>,
        /// Questions skipped.
        skipped: HashSet<usize>,
        /// Questions answered or skipped.
        finished: HashSet<usize>,
        /// (chat, message) of each question's own message.
        messages: Vec<Vec<(i64, i64)>>,
    },
}

#[derive(Clone)]
struct Question {
    text: String,
    header: String,
    options: Vec<String>,
    multi: bool,
}

pub struct Telegram {
    db: Arc<Db>,
    bots: Arc<Bots>,
    agents: Arc<Agents>,
    sink: Sink,
    api: Mutex<Option<Arc<dyn Api>>>,
    username: Mutex<Option<String>>,
    error: Mutex<Option<String>>,
    /// Bumped when the token changes, so an old polling thread stops.
    generation: AtomicU64,
    pairing: Mutex<Option<Pairing>>,
    /// Prompt token → the request (tokens go in button data, which holds 64 bytes).
    prompts: Mutex<HashMap<String, Prompt>>,
    /// Tasks already reported (a task is published again when its handed-on tasks change).
    reported: Mutex<HashSet<String>>,
    /// Unpaired chats already told the bot is private (once each).
    told: Mutex<HashSet<i64>>,
}

impl Telegram {
    pub fn new(db: Arc<Db>, bots: Arc<Bots>, agents: Arc<Agents>, sink: Sink) -> Self {
        Self {
            db,
            bots,
            agents,
            sink,
            api: Mutex::new(None),
            username: Mutex::new(None),
            error: Mutex::new(None),
            generation: AtomicU64::new(0),
            pairing: Mutex::new(None),
            prompts: Mutex::new(HashMap::new()),
            reported: Mutex::new(HashSet::new()),
            told: Mutex::new(HashSet::new()),
        }
    }

    fn api(&self) -> Option<Arc<dyn Api>> {
        self.api.lock().ok().and_then(|a| a.clone())
    }

    /* ------------------------------------------------------------ settings */

    pub fn status(&self) -> Status {
        let pairing = self.pairing.lock().ok();
        let (code, until) = pairing
            .as_ref()
            .and_then(|p| p.as_ref())
            .filter(|p| p.until > now_ms())
            .map(|p| (Some(p.code.clone()), Some(p.until)))
            .unwrap_or((None, None));
        Status {
            connected: self.api().is_some(),
            username: self.username.lock().ok().and_then(|u| u.clone()),
            error: self.error.lock().ok().and_then(|e| e.clone()),
            chats: self.chats(),
            pairing_code: code,
            pairing_until: until,
            notify: self.notify(),
        }
    }

    fn publish(&self) {
        (self.sink)(Outbound::Telegram(self.status()));
    }

    fn set_error(&self, e: Option<String>) {
        let changed = self
            .error
            .lock()
            .map(|mut x| {
                let c = *x != e;
                *x = e;
                c
            })
            .unwrap_or(false);
        if changed {
            self.publish();
        }
    }

    pub fn chats(&self) -> Vec<Chat> {
        self.db
            .meta(CHATS)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_chats(&self, chats: &[Chat]) {
        let _ = self
            .db
            .set_meta(CHATS, &serde_json::to_string(chats).unwrap_or_default());
    }

    fn paired(&self, chat: i64) -> bool {
        self.chats().iter().any(|c| c.id == chat)
    }

    pub fn notify(&self) -> String {
        self.db
            .meta(NOTIFY)
            .ok()
            .flatten()
            .filter(|n| matches!(n.as_str(), "telegram" | "schedules" | "all"))
            .unwrap_or_else(|| "schedules".into())
    }

    pub fn set_notify(&self, n: &str) -> Result<Status, String> {
        if !matches!(n, "telegram" | "schedules" | "all") {
            return Err("Unknown choice".into());
        }
        self.db.set_meta(NOTIFY, n).map_err(|e| e.to_string())?;
        self.publish();
        Ok(self.status())
    }

    /// Saves a token after Telegram accepted it, and starts listening.
    pub fn connect(self: &Arc<Self>, token: &str) -> Result<Status, String> {
        let token = token.trim();
        let ok = token.split_once(':').is_some_and(|(id, secret)| {
            !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) && secret.len() >= 20
        });
        if !ok {
            return Err("That doesn't look like a bot token. It looks like 123456789:AA… and comes from @BotFather.".into());
        }
        let api: Arc<dyn Api> = Arc::new(Http::new(token)?);
        let name = api
            .get_me()
            .map_err(|e| format!("Telegram didn't accept that token: {e}"))?;
        crate::secrets::set_named(TOKEN, token)?;
        // Another bot's updates don't continue this one's.
        let _ = self.db.set_meta(OFFSET, "0");
        self.start_with(api, Some(name));
        Ok(self.status())
    }

    /// Starts with the token in the keychain, if there is one (at launch).
    pub fn start_saved(self: &Arc<Self>) {
        if let Some(token) = crate::secrets::get_named(TOKEN) {
            match Http::new(&token) {
                Ok(api) => self.start_with(Arc::new(api), None),
                Err(e) => self.set_error(Some(e)),
            }
        }
    }

    pub fn disconnect(&self) -> Status {
        crate::secrets::remove_named(TOKEN);
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut a) = self.api.lock() {
            *a = None;
        }
        if let Ok(mut u) = self.username.lock() {
            *u = None;
        }
        if let Ok(mut e) = self.error.lock() {
            *e = None;
        }
        // Chats were paired with that bot.
        self.save_chats(&[]);
        let _ = self.db.set_meta(OFFSET, "0");
        self.publish();
        self.status()
    }

    pub fn start_pairing(&self) -> Result<Status, String> {
        if self.api().is_none() {
            return Err("Connect your Telegram bot first".into());
        }
        let n = uuid::Uuid::new_v4().as_u128() % 1_000_000;
        if let Ok(mut p) = self.pairing.lock() {
            *p = Some(Pairing {
                code: format!("{n:06}"),
                until: now_ms() + PAIR_MINUTES * 60_000,
                tries: 0,
            });
        }
        self.publish();
        Ok(self.status())
    }

    pub fn unpair(&self, chat: i64) -> Status {
        let chats: Vec<Chat> = self.chats().into_iter().filter(|c| c.id != chat).collect();
        self.save_chats(&chats);
        self.publish();
        self.status()
    }

    /// Uses `api` from now on (tests pass a fake one) and listens for updates.
    pub fn start_with(self: &Arc<Self>, api: Arc<dyn Api>, username: Option<String>) {
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut a) = self.api.lock() {
            *a = Some(api.clone());
        }
        if let Ok(mut u) = self.username.lock() {
            *u = username;
        }
        self.publish();
        let me = self.clone();
        std::thread::spawn(move || me.poll(api, gen));
    }

    fn poll(&self, api: Arc<dyn Api>, gen: u64) {
        let mut wait = 2u64;
        if self.username.lock().map(|u| u.is_none()).unwrap_or(true) {
            if let Ok(name) = api.get_me() {
                if let Ok(mut u) = self.username.lock() {
                    *u = Some(name);
                }
                self.publish();
            }
        }
        while self.generation.load(Ordering::SeqCst) == gen {
            let offset = self
                .db
                .meta(OFFSET)
                .ok()
                .flatten()
                .and_then(|o| o.parse::<i64>().ok())
                .unwrap_or(0);
            match api.get_updates(offset, 25) {
                Ok(updates) => {
                    wait = 2;
                    self.set_error(None);
                    for u in updates {
                        if self.generation.load(Ordering::SeqCst) != gen {
                            return;
                        }
                        if let Some(id) = u["update_id"].as_i64() {
                            let _ = self.db.set_meta(OFFSET, &(id + 1).to_string());
                        }
                        self.handle(&u);
                    }
                }
                Err(e) => {
                    self.set_error(Some(e));
                    std::thread::sleep(Duration::from_secs(wait));
                    wait = (wait * 2).min(60);
                }
            }
        }
    }

    /* ---------------------------------------------------------- from Telegram */

    /// One update from Telegram: a message or a button press.
    pub fn handle(&self, u: &Value) {
        if let Some(m) = u.get("message") {
            self.on_message(m);
        } else if let Some(c) = u.get("callback_query") {
            self.on_button(c);
        }
    }

    /// A message to every paired chat (a bot's `notify_me`, a trigger that fired).
    pub fn tell_all(&self, text: &str) {
        for c in self.chats() {
            self.say(c.id, text);
        }
    }

    fn say(&self, chat: i64, text: &str) {
        self.say_with(chat, text, None, None);
    }

    fn say_with(
        &self,
        chat: i64,
        text: &str,
        keyboard: Option<Value>,
        reply_to: Option<i64>,
    ) -> Option<i64> {
        let api = self.api()?;
        let parts = chunks(text);
        let last = parts.len().saturating_sub(1);
        let mut id = None;
        for (i, part) in parts.iter().enumerate() {
            let kb = if i == last { keyboard.clone() } else { None };
            let r = if i == 0 { reply_to } else { None };
            match api.send(chat, part, kb, r) {
                Ok(m) => id = Some(m),
                Err(e) => {
                    self.set_error(Some(e));
                    return None;
                }
            }
        }
        id
    }

    fn on_message(&self, m: &Value) {
        let Some(chat) = m.pointer("/chat/id").and_then(Value::as_i64) else {
            return;
        };
        // Private chats only: in a group everyone in it could give your bots work.
        if m.pointer("/chat/type").and_then(Value::as_str) != Some("private") {
            return;
        }
        let msg_id = m["message_id"].as_i64();
        let text = m["text"]
            .as_str()
            .or_else(|| m["caption"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let (cmd, rest) = command(&text);
        if !self.paired(chat) {
            if cmd == "pair" {
                self.pair(chat, rest, m);
            } else if self
                .told
                .lock()
                .map(|mut t| {
                    if t.len() > 1000 {
                        t.clear();
                    }
                    t.insert(chat)
                })
                .unwrap_or(false)
            {
                self.say(chat, "This bot belongs to someone's AgentDeck and only answers chats they paired. If it's yours: in AgentDeck open Settings → Telegram, press Pair a chat, and send the code here as /pair 123456.");
            }
            return;
        }
        if text.is_empty() {
            self.say(
                chat,
                "I can only read text for now. Type what you'd like the bot to do.",
            );
            return;
        }
        // A reply to a question's message is your own answer to it.
        if let Some(to) = m
            .pointer("/reply_to_message/message_id")
            .and_then(Value::as_i64)
        {
            if self.answer_in_words(chat, to, &text) {
                return;
            }
        }
        match cmd {
            "start" | "help" => self.say(chat, &help(&self.bot_names())),
            "pair" => self.say(chat, "This chat is already paired."),
            "bots" => self.list_bots(chat),
            "use" => self.use_bot(chat, rest),
            "status" => self.bot_status(chat),
            "cancel" => self.cancel(chat),
            "unpair" => {
                self.unpair(chat);
                self.say(chat, "Unpaired. This chat won't reach your bots anymore.");
            }
            "" => self.task(chat, &text, msg_id),
            other => self.say(
                chat,
                &format!("I don't know /{other}. Send /help to see what I can do."),
            ),
        }
    }

    fn pair(&self, chat: i64, code: &str, m: &Value) {
        let code = code.trim();
        let mut ok = false;
        if let Ok(mut p) = self.pairing.lock() {
            if let Some(x) = p.as_mut() {
                if x.until > now_ms() && x.code == code && !code.is_empty() {
                    ok = true;
                    *p = None;
                } else {
                    x.tries += 1;
                    if x.tries >= PAIR_TRIES || x.until <= now_ms() {
                        *p = None;
                    }
                }
            }
        }
        if !ok {
            self.say(chat, "That code didn't work. In AgentDeck open Settings → Telegram and press Pair a chat for a new one.");
            self.publish();
            return;
        }
        let first = m
            .pointer("/from/first_name")
            .and_then(Value::as_str)
            .unwrap_or("");
        let user = m.pointer("/from/username").and_then(Value::as_str);
        let name = match user {
            Some(u) if !first.is_empty() => format!("{first} (@{u})"),
            Some(u) => format!("@{u}"),
            None if !first.is_empty() => first.to_string(),
            None => format!("Chat {chat}"),
        };
        let mut chats = self.chats();
        chats.retain(|c| c.id != chat);
        chats.push(Chat {
            id: chat,
            name,
            paired_at: now_ms(),
        });
        self.save_chats(&chats);
        self.publish();
        self.say(chat, &format!("Paired. {}", help(&self.bot_names())));
    }

    fn bot_names(&self) -> Vec<String> {
        self.bots
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|b| b.name)
            .collect()
    }

    fn current_key(chat: i64) -> String {
        format!("telegram.bot.{chat}")
    }

    /// The bot a chat talks to: the one it picked, or the only one there is.
    fn current(&self, chat: i64, bots: &[Bot]) -> Option<Bot> {
        let picked = self.db.meta(&Self::current_key(chat)).ok().flatten();
        picked
            .and_then(|id| bots.iter().find(|b| b.id == id).cloned())
            .or_else(|| (bots.len() == 1).then(|| bots[0].clone()))
    }

    fn pick_keyboard(bots: &[Bot], action: &str) -> Value {
        Value::Array(
            bots.iter()
                .map(|b| json!([{ "text": b.name, "callback_data": format!("{action}:{}", b.id) }]))
                .collect(),
        )
    }

    fn list_bots(&self, chat: i64) {
        let bots = self.bots.list().unwrap_or_default();
        if bots.is_empty() {
            self.say(
                chat,
                "You have no bots yet. Make one in AgentDeck (Bots → New bot).",
            );
            return;
        }
        let current = self.current(chat, &bots).map(|b| b.id);
        let mut text = String::from("Your bots:\n");
        for b in &bots {
            text.push_str(&format!(
                "\n{} {}: {}",
                if Some(&b.id) == current.as_ref() {
                    "▸"
                } else {
                    "•"
                },
                b.name,
                state(b)
            ));
        }
        text.push_str("\n\nPick the one your messages go to:");
        self.say_with(chat, &text, Some(Self::pick_keyboard(&bots, "use")), None);
    }

    fn use_bot(&self, chat: i64, name: &str) {
        let bots = self.bots.list().unwrap_or_default();
        if name.trim().is_empty() {
            self.list_bots(chat);
            return;
        }
        match find_bot(&bots, name) {
            Some(b) => {
                let _ = self.db.set_meta(&Self::current_key(chat), &b.id);
                self.say(chat, &format!("Your messages now go to {}.", b.name));
            }
            None => self.say(
                chat,
                &format!(
                    "There's no bot called {}. Send /bots to see them.",
                    name.trim()
                ),
            ),
        }
    }

    fn bot_status(&self, chat: i64) {
        let bots = self.bots.list().unwrap_or_default();
        match self.current(chat, &bots) {
            Some(b) => self.say(chat, &format!("{}: {}", b.name, state(&b))),
            None => self.list_bots(chat),
        }
    }

    fn cancel(&self, chat: i64) {
        let bots = self.bots.list().unwrap_or_default();
        let Some(b) = self.current(chat, &bots) else {
            self.list_bots(chat);
            return;
        };
        match b.running_task_id.as_deref() {
            Some(t) => match self.bots.cancel(t) {
                Ok(_) => self.say(chat, &format!("Stopped {}'s task.", b.name)),
                Err(e) => self.say(chat, &e),
            },
            None => self.say(chat, &format!("{} isn't working on anything.", b.name)),
        }
    }

    /// A message: a task for a bot ("Reviewer: …" or "@Reviewer …" picks one).
    fn task(&self, chat: i64, text: &str, msg_id: Option<i64>) {
        let bots = self.bots.list().unwrap_or_default();
        if bots.is_empty() {
            self.say(
                chat,
                "You have no bots yet. Make one in AgentDeck (Bots → New bot).",
            );
            return;
        }
        let (bot, text) = match addressed(&bots, text) {
            Some((b, rest)) => (Some(b), rest),
            None => (self.current(chat, &bots), text.to_string()),
        };
        let Some(bot) = bot else {
            self.say_with(
                chat,
                "Which bot is this for? (Next time you can start with its name, like \"Reviewer: …\".)",
                Some(Self::pick_keyboard(&bots, "use")),
                msg_id,
            );
            return;
        };
        if text.trim().is_empty() {
            self.say(chat, &format!("What should {} do?", bot.name));
            return;
        }
        let source = format!("telegram:{chat}:{}", msg_id.unwrap_or(0));
        match self.bots.add_task(&bot.id, &text, &source, None) {
            Ok(t) => {
                let ahead = bot.queued + i64::from(bot.running_task_id.is_some());
                let note = if t.status == "running" || ahead == 0 {
                    format!("{} is on it.", bot.name)
                } else if bot.paused {
                    format!(
                        "Queued for {}. It's paused, so it starts when you resume it.",
                        bot.name
                    )
                } else {
                    format!("Queued for {} ({ahead} ahead).", bot.name)
                };
                self.say_with(chat, &note, None, msg_id);
            }
            Err(e) => {
                self.say_with(
                    chat,
                    &format!("Couldn't give it to {}: {e}", bot.name),
                    None,
                    msg_id,
                );
            }
        }
    }

    fn on_button(&self, c: &Value) {
        let id = c["id"].as_str().unwrap_or("");
        let chat = c.pointer("/message/chat/id").and_then(Value::as_i64);
        let data = c["data"].as_str().unwrap_or("");
        let reply = |text: &str| {
            if let Some(api) = self.api() {
                let _ = api.answer_callback(id, text);
            }
        };
        let Some(chat) = chat.filter(|c| self.paired(*c)) else {
            reply("This chat isn't paired");
            return;
        };
        if let Some(bot_id) = data.strip_prefix("use:") {
            let bots = self.bots.list().unwrap_or_default();
            match bots.iter().find(|b| b.id == bot_id) {
                Some(b) => {
                    let _ = self.db.set_meta(&Self::current_key(chat), &b.id);
                    reply(&format!("Messages go to {}", b.name));
                    self.say(
                        chat,
                        &format!("Your messages now go to {}. What should it do?", b.name),
                    );
                }
                None => reply("That bot isn't there anymore"),
            }
            return;
        }
        let Some((token, action)) = data.split_once(':') else {
            reply("");
            return;
        };
        reply(&self.act(token, action));
    }

    /// A button on a request: what it did, for the little note Telegram shows.
    fn act(&self, token: &str, action: &str) -> String {
        let mut prompts = match self.prompts.lock() {
            Ok(p) => p,
            Err(_) => return String::new(),
        };
        let Some(p) = prompts.get_mut(token) else {
            return "This was answered already".into();
        };
        match (&mut p.kind, action) {
            (PromptKind::Approval | PromptKind::Plan, "a" | "d") => {
                let allow = action == "a";
                let d = if allow {
                    Decision::Allow
                } else {
                    Decision::Deny
                };
                match self.agents.respond(&p.session_id, &p.request_id, d) {
                    Ok(()) => {
                        let done = match (&p.kind, allow) {
                            (PromptKind::Plan, true) => "✅ Plan approved",
                            (PromptKind::Plan, false) => "✋ Kept planning",
                            (_, true) => "✅ Allowed",
                            _ => "❌ Denied",
                        };
                        let p = prompts.remove(token).unwrap();
                        drop(prompts);
                        self.close(&p, done);
                        done.trim_start_matches(|c: char| !c.is_alphabetic())
                            .to_string()
                    }
                    Err(e) => {
                        let p = prompts.remove(token).unwrap();
                        drop(prompts);
                        self.close(&p, "No longer waiting");
                        e
                    }
                }
            }
            (
                PromptKind::Question {
                    questions,
                    answers,
                    skipped,
                    finished,
                    messages,
                },
                a,
            ) => {
                // "o<q>.<option>", "k<q>" (done choosing), "s<q>" (skip).
                let (verb, rest) = a.split_at(1.min(a.len()));
                let (qi, oi) = match rest.split_once('.') {
                    Some((q, o)) => (q.parse::<usize>().ok(), o.parse::<usize>().ok()),
                    None => (rest.parse::<usize>().ok(), None),
                };
                let Some(qi) = qi.filter(|q| *q < questions.len()) else {
                    return String::new();
                };
                if finished.contains(&qi) {
                    return "That one is answered".into();
                }
                let q = questions[qi].clone();
                match (verb, oi) {
                    ("o", Some(oi)) if oi < q.options.len() => {
                        let label = q.options[oi].clone();
                        let chosen = answers.entry(q.text.clone()).or_default();
                        if q.multi {
                            if let Some(i) = chosen.iter().position(|c| *c == label) {
                                chosen.remove(i);
                            } else {
                                chosen.push(label);
                            }
                        } else {
                            *chosen = vec![label];
                            finished.insert(qi);
                        }
                    }
                    ("k", _) => {
                        if answers.get(&q.text).is_some_and(|c| !c.is_empty()) {
                            finished.insert(qi);
                        } else {
                            return "Pick at least one, or Skip".into();
                        }
                    }
                    ("s", _) => {
                        answers.remove(&q.text);
                        skipped.insert(qi);
                        finished.insert(qi);
                    }
                    _ => return String::new(),
                }
                let chosen = answers.get(&q.text).cloned().unwrap_or_default();
                let done = finished.contains(&qi);
                let text = question_text(&p.bot_name, &q, &chosen, done, skipped.contains(&qi));
                let kb = (!done).then(|| question_keyboard(token, qi, &q, &chosen));
                if let Some(api) = self.api() {
                    for (c, m) in &messages[qi] {
                        let _ = api.edit(*c, *m, &text, kb.clone());
                    }
                }
                if finished.len() < questions.len() {
                    return if done { "Got it".into() } else { String::new() };
                }
                // Every question has its answer (or was skipped): they go to the agent.
                answers.retain(|_, v| !v.is_empty());
                let answers = answers.clone();
                let p = prompts.remove(token).unwrap();
                drop(prompts);
                let res = if answers.is_empty() {
                    self.agents.answer(&p.session_id, &p.request_id, None)
                } else {
                    self.agents
                        .answer(&p.session_id, &p.request_id, Some(&answers))
                };
                match res {
                    Ok(()) => "Sent".into(),
                    Err(e) => e,
                }
            }
            _ => String::new(),
        }
    }

    /// A reply to a question's message: your own answer to that question.
    fn answer_in_words(&self, chat: i64, message: i64, text: &str) -> bool {
        let token = {
            let prompts = match self.prompts.lock() {
                Ok(p) => p,
                Err(_) => return false,
            };
            prompts.iter().find_map(|(tok, p)| match &p.kind {
                PromptKind::Question { messages, .. } => messages
                    .iter()
                    .position(|ms| ms.contains(&(chat, message)))
                    .map(|qi| (tok.clone(), qi)),
                _ => None,
            })
        };
        let Some((tok, qi)) = token else {
            return false;
        };
        if let Ok(mut prompts) = self.prompts.lock() {
            if let Some(Prompt {
                kind:
                    PromptKind::Question {
                        questions,
                        answers,
                        finished,
                        ..
                    },
                ..
            }) = prompts.get_mut(&tok)
            {
                // An answered question keeps its answer.
                if finished.contains(&qi) {
                    drop(prompts);
                    self.say(chat, "That question is answered already.");
                    return true;
                }
                let q = questions[qi].text.clone();
                answers.insert(q, vec![text.to_string()]);
            }
        }
        let note = self.act(&tok, &format!("k{qi}"));
        if !note.is_empty() && note != "Got it" && note != "Sent" {
            self.say(chat, &note);
        }
        true
    }

    /// The request was answered: its messages say how, and lose their buttons.
    fn close(&self, p: &Prompt, how: &str) {
        let Some(api) = self.api() else { return };
        let text = format!("{}\n\n{how}", p.text);
        for (c, m) in &p.sent {
            let _ = api.edit(*c, *m, &text, None);
        }
        if let PromptKind::Question { messages, .. } = &p.kind {
            for ms in messages {
                for (c, m) in ms {
                    let _ = api.edit(*c, *m, &format!("{}\n\n{how}", p.text), None);
                }
            }
        }
    }

    /* ---------------------------------------------------------- to Telegram */

    /// A stored event: a bot needing you, or a request answered (in AgentDeck or here).
    pub fn on_event(&self, ev: &StoredEvent) {
        match ev.kind.as_str() {
            "bot_needs_you" if ev.source == "agentdeck" => self.needs_you(ev),
            "permission_decision" | "question_answered" => {
                let Some(rid) = ev.payload["requestId"].as_str() else {
                    return;
                };
                let p = self.prompts.lock().ok().and_then(|mut ps| {
                    let tok = ps
                        .iter()
                        .find(|(_, p)| p.request_id == rid && p.session_id == ev.session_id)
                        .map(|(t, _)| t.clone())?;
                    ps.remove(&tok)
                });
                if let Some(p) = p {
                    let how = match (ev.kind.as_str(), ev.payload["decision"].as_str()) {
                        ("question_answered", _) => "Answered",
                        (_, Some("deny")) => "❌ Denied",
                        _ => "✅ Allowed",
                    };
                    self.close(&p, how);
                }
            }
            _ => {}
        }
    }

    fn needs_you(&self, ev: &StoredEvent) {
        let chats = self.chats();
        if chats.is_empty() || self.api().is_none() {
            return;
        }
        let p = &ev.payload;
        let (Some(rid), Some(bot_id)) = (p["requestId"].as_str(), p["botId"].as_str()) else {
            return;
        };
        let bot = self.db.bot(bot_id).ok().flatten();
        let bot_name = bot.map(|b| b.name).unwrap_or_else(|| "A bot".into());
        let task = p["taskId"]
            .as_str()
            .and_then(|t| self.db.bot_task(t).ok().flatten())
            .map(|t| first_line(&t.text, 120))
            .unwrap_or_default();
        // The request itself: the newest event of the session with that id.
        let events = self.db.events(&ev.session_id).unwrap_or_default();
        let Some(req) = events.iter().rev().find(|e| {
            crate::unattended::request_id(e).as_deref() == Some(rid) && e.source != "agentdeck"
        }) else {
            return;
        };
        let token = short_token();
        let tool = crate::unattended::claude_tool(req).unwrap_or("");
        let input = req
            .payload
            .pointer("/request/input")
            .cloned()
            .unwrap_or_else(|| req.payload.clone());
        let task_line = if task.is_empty() {
            String::new()
        } else {
            format!("\nTask: {task}")
        };
        let mut prompt = Prompt {
            session_id: ev.session_id.clone(),
            request_id: rid.to_string(),
            task_id: p["taskId"].as_str().map(str::to_owned),
            bot_name: bot_name.clone(),
            text: String::new(),
            sent: Vec::new(),
            kind: PromptKind::Approval,
        };
        // What goes out: (question index or None, text, buttons).
        let mut outgoing: Vec<(Option<usize>, String, Value)> = Vec::new();
        if tool == "AskUserQuestion" {
            let questions: Vec<Question> = input["questions"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|q| Question {
                    text: q["question"].as_str().unwrap_or("").to_string(),
                    header: q["header"].as_str().unwrap_or("").to_string(),
                    options: q["options"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|o| o["label"].as_str().map(str::to_owned))
                        .collect(),
                    multi: q["multiSelect"].as_bool().unwrap_or(false),
                })
                .filter(|q| !q.text.is_empty())
                .collect();
            if questions.is_empty() {
                return;
            }
            prompt.text = format!("❓ {bot_name} has a question{task_line}");
            for (qi, q) in questions.iter().enumerate() {
                outgoing.push((
                    Some(qi),
                    question_text(&bot_name, q, &[], false, false),
                    question_keyboard(&token, qi, q, &[]),
                ));
            }
            prompt.kind = PromptKind::Question {
                messages: vec![Vec::new(); questions.len()],
                questions,
                answers: HashMap::new(),
                skipped: HashSet::new(),
                finished: HashSet::new(),
            };
        } else if tool == "ExitPlanMode" {
            let plan = input["plan"].as_str().unwrap_or("");
            prompt.kind = PromptKind::Plan;
            prompt.text = format!(
                "📋 {bot_name} has a plan for you to approve{task_line}\n\n{}",
                clip(plan, 3000)
            );
            outgoing.push((
                None,
                prompt.text.clone(),
                json!([[
                    { "text": "✅ Approve plan", "callback_data": format!("{token}:a") },
                    { "text": "✋ Keep planning", "callback_data": format!("{token}:d") }
                ]]),
            ));
        } else if p["question"].as_bool() == Some(true) {
            // A question this agent asks in its own way: answered in AgentDeck.
            let text = format!("❓ {bot_name} has a question for you in AgentDeck.{task_line}");
            for c in &chats {
                self.say(c.id, &text);
            }
            return;
        } else {
            let why = p["why"].as_str().unwrap_or("it looked risky");
            prompt.text = format!(
                "🔔 {bot_name} needs you{task_line}\n\nIt wants to: {}\n\nIt waited because {why}.",
                describe(tool, &input)
            );
            outgoing.push((
                None,
                prompt.text.clone(),
                json!([[
                    { "text": "✅ Allow", "callback_data": format!("{token}:a") },
                    { "text": "❌ Deny", "callback_data": format!("{token}:d") }
                ]]),
            ));
        }
        // Kept before it's sent, so a quick tap (or an answer in AgentDeck) finds it.
        if let Ok(mut ps) = self.prompts.lock() {
            ps.insert(token.clone(), prompt);
        }
        for (qi, text, kb) in outgoing {
            for c in &chats {
                let Some(m) = self.say_with(c.id, &text, Some(kb.clone()), None) else {
                    continue;
                };
                let mut ps = match self.prompts.lock() {
                    Ok(ps) => ps,
                    Err(_) => return,
                };
                match ps.get_mut(&token) {
                    Some(Prompt {
                        kind: PromptKind::Question { messages, .. },
                        ..
                    }) => {
                        if let Some(ms) = qi.and_then(|i| messages.get_mut(i)) {
                            ms.push((c.id, m));
                        }
                    }
                    Some(p) => p.sent.push((c.id, m)),
                    // Answered meanwhile: this message says so at once.
                    None => {
                        drop(ps);
                        if let Some(api) = self.api() {
                            let _ = api.edit(c.id, m, &format!("{text}\n\nAnswered"), None);
                        }
                    }
                }
            }
        }
    }

    /// A bot task changed: a finished one is reported where it should be.
    pub fn on_task(&self, t: &BotTask) {
        if !matches!(t.status.as_str(), "done" | "failed" | "cancelled") || self.api().is_none() {
            return;
        }
        // Its requests can't be answered anymore.
        let over: Vec<Prompt> = self
            .prompts
            .lock()
            .map(|mut ps| {
                let toks: Vec<String> = ps
                    .iter()
                    .filter(|(_, p)| p.task_id.as_deref() == Some(t.id.as_str()))
                    .map(|(k, _)| k.clone())
                    .collect();
                toks.iter().filter_map(|k| ps.remove(k)).collect()
            })
            .unwrap_or_default();
        for p in &over {
            self.close(p, "No longer waiting: the task ended");
        }
        let fresh = self
            .reported
            .lock()
            .map(|mut r| {
                if r.len() > 5000 {
                    r.clear();
                }
                r.insert(t.id.clone())
            })
            .unwrap_or(false);
        if !fresh {
            return;
        }
        let chats = self.chats();
        let from_tg = t.source.strip_prefix("telegram:").map(|rest| {
            let mut it = rest.split(':');
            let chat = it.next().and_then(|c| c.parse::<i64>().ok());
            let msg = it
                .next()
                .and_then(|m| m.parse::<i64>().ok())
                .filter(|m| *m > 0);
            (chat, msg)
        });
        let to: Vec<(i64, Option<i64>)> = match from_tg {
            Some((Some(chat), msg)) => chats
                .iter()
                .filter(|c| c.id == chat)
                .map(|c| (c.id, msg))
                .collect(),
            Some(_) => Vec::new(),
            None => {
                // Cancelled tasks are only reported where they were sent from.
                let n = self.notify();
                let wanted = t.status != "cancelled"
                    && (n == "all"
                        || (n == "schedules"
                            && (t.source == "schedule" || t.source.starts_with("trigger:"))));
                if wanted {
                    chats.iter().map(|c| (c.id, None)).collect()
                } else {
                    Vec::new()
                }
            }
        };
        if to.is_empty() {
            return;
        }
        let name = self
            .db
            .bot(&t.bot_id)
            .ok()
            .flatten()
            .map(|b| b.name)
            .unwrap_or_else(|| "The bot".into());
        let head = if t.source == "schedule" {
            format!(" (scheduled: {})", first_line(&t.text, 60))
        } else if t.source.starts_with("trigger:") {
            format!(" (a trigger fired: {})", first_line(&t.text, 60))
        } else if from_tg.is_none() {
            format!(" ({})", first_line(&t.text, 60))
        } else {
            String::new()
        };
        let text = match t.status.as_str() {
            "done" => {
                let r = t.result.as_deref().unwrap_or("").trim();
                if r.is_empty() {
                    format!("✅ {name} is done{head}.")
                } else {
                    format!("✅ {name}{head}:\n\n{r}")
                }
            }
            "failed" => format!(
                "⚠️ {name} couldn't finish{head}: {}",
                t.error.as_deref().unwrap_or("something went wrong")
            ),
            _ => format!("{name}'s task was cancelled{head}."),
        };
        for (chat, reply) in to {
            self.say_with(chat, &text, None, reply);
        }
    }
}

/* ------------------------------------------------------------------ helpers */

/// "/use Reviewer" → ("use", "Reviewer"); "/use@MyBot x" → ("use", "x"); text → ("", text).
fn command(text: &str) -> (&str, &str) {
    let Some(rest) = text.strip_prefix('/') else {
        return ("", text);
    };
    let (head, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let name = head.split('@').next().unwrap_or("");
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return ("", text);
    }
    (name, tail.trim())
}

fn find_bot(bots: &[Bot], name: &str) -> Option<Bot> {
    let n = name.trim().trim_start_matches('@').to_lowercase();
    if n.is_empty() {
        return None;
    }
    bots.iter()
        .find(|b| b.name.to_lowercase() == n)
        .or_else(|| {
            let starts: Vec<&Bot> = bots
                .iter()
                .filter(|b| b.name.to_lowercase().starts_with(&n))
                .collect();
            (starts.len() == 1).then(|| starts[0])
        })
        .cloned()
}

/// `text` without `prefix` at its start, letters compared without case (Turkish İ/ı, ß… too),
/// cut at a character boundary of `text` itself.
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut rest = text.char_indices();
    for pc in prefix.chars() {
        let (_, tc) = rest.next()?;
        if !pc.to_lowercase().eq(tc.to_lowercase()) {
            return None;
        }
    }
    Some(match rest.next() {
        Some((i, _)) => &text[i..],
        None => "",
    })
}

/// "Reviewer: look at PR 12" or "@Reviewer look at PR 12": that bot, and the rest.
fn addressed(bots: &[Bot], text: &str) -> Option<(Bot, String)> {
    let t = text.trim();
    let mut by_len: Vec<&Bot> = bots.iter().collect();
    by_len.sort_by_key(|b| std::cmp::Reverse(b.name.chars().count()));
    for b in by_len {
        let name = b.name.trim();
        if name.is_empty() {
            continue;
        }
        let tagged = t.strip_prefix('@').and_then(|x| strip_prefix_ci(x, name));
        if let Some(rest) = tagged {
            if rest.is_empty()
                || rest.starts_with(char::is_whitespace)
                || rest.starts_with([':', ','])
            {
                return Some((
                    b.clone(),
                    rest.trim_start_matches([':', ',']).trim().to_string(),
                ));
            }
        }
        if let Some(rest) = strip_prefix_ci(t, name) {
            if rest.starts_with([':', ',']) {
                return Some((
                    b.clone(),
                    rest.trim_start_matches([':', ',']).trim().to_string(),
                ));
            }
        }
    }
    None
}

fn state(b: &Bot) -> String {
    let mut s = if b.needs_you {
        "needs you".to_string()
    } else if let Some(t) = &b.running_text {
        format!("working on \"{}\"", first_line(t, 60))
    } else if b.paused {
        "paused".into()
    } else {
        "free".into()
    };
    if b.queued > 0 {
        s.push_str(&format!(", {} queued", b.queued));
    }
    s
}

fn help(bots: &[String]) -> String {
    let example = bots.first().map(String::as_str).unwrap_or("Reviewer");
    format!(
        "Write what you'd like done and the bot you picked gets it as a task; its answer comes back here. Start with a bot's name to pick another: \"{example}: …\".\n\n/bots: your bots, and pick the one your messages go to\n/status: what it's doing\n/cancel: stop its task\n/unpair: stop this chat reaching your bots\n\nWhen a bot needs you (a risky step, a question, a plan), it asks here with buttons."
    )
}

fn first_line(s: &str, max: usize) -> String {
    let line = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    clip(line, max)
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut.trim_end())
    }
}

/// Splits a long text into Telegram-sized messages, at line breaks where it can.
fn chunks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.split_inclusive('\n') {
        if cur.chars().count() + line.chars().count() > CHUNK && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if line.chars().count() > CHUNK {
            let chars: Vec<char> = line.chars().collect();
            for piece in chars.chunks(CHUNK) {
                out.push(piece.iter().collect());
            }
        } else {
            cur.push_str(line);
        }
    }
    if !cur.trim().is_empty() || out.is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|s| s.trim_end().to_string()).collect()
}

/// What a request wants, in a line: the command, the file, the address.
fn describe(tool: &str, input: &Value) -> String {
    let pick = |keys: &[&str]| {
        keys.iter().find_map(|k| match &input[*k] {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::Array(a) if !a.is_empty() => Some(
                a.iter()
                    .map(|x| {
                        x.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| x.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            _ => None,
        })
    };
    let what = match tool {
        "Bash" => pick(&["command"]).map(|c| format!("run {}", clip(&c, 600))),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            pick(&["file_path", "notebook_path"]).map(|f| format!("change {f}"))
        }
        "Read" => pick(&["file_path"]).map(|f| format!("read {f}")),
        "WebFetch" => pick(&["url"]).map(|u| format!("open {u}")),
        _ => None,
    };
    what.or_else(|| {
        pick(&["command", "cmd"])
            .map(|c| format!("run {}", clip(&c, 600)))
            .or_else(|| pick(&["path", "file_path"]).map(|f| format!("change {f}")))
    })
    .unwrap_or_else(|| {
        let name = if tool.is_empty() { "a tool" } else { tool };
        format!("use {name}: {}", clip(&input.to_string(), 400))
    })
}

fn question_text(bot: &str, q: &Question, chosen: &[String], done: bool, skipped: bool) -> String {
    let mut s = format!("❓ {bot} asks");
    if !q.header.is_empty() {
        s.push_str(&format!(" ({})", q.header));
    }
    s.push_str(&format!(":\n\n{}", q.text));
    if skipped {
        s.push_str("\n\nSkipped.");
    } else if done && !chosen.is_empty() {
        s.push_str(&format!("\n\nYour answer: {}", chosen.join(", ")));
    } else if q.multi {
        s.push_str("\n\nPick any, then Done. Or reply to this message with your own answer.");
    } else {
        s.push_str("\n\nPick one, or reply to this message with your own answer.");
    }
    s
}

fn question_keyboard(token: &str, qi: usize, q: &Question, chosen: &[String]) -> Value {
    let mut rows: Vec<Value> = q
        .options
        .iter()
        .enumerate()
        .map(|(oi, o)| {
            let mark = if chosen.contains(o) { "✓ " } else { "" };
            json!([{ "text": format!("{mark}{}", clip(o, 60)), "callback_data": format!("{token}:o{qi}.{oi}") }])
        })
        .collect();
    let mut last = Vec::new();
    if q.multi {
        last.push(json!({ "text": "Done", "callback_data": format!("{token}:k{qi}") }));
    }
    last.push(json!({ "text": "Skip", "callback_data": format!("{token}:s{qi}") }));
    rows.push(Value::Array(last));
    Value::Array(rows)
}

fn short_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    type Sent = (i64, String, Option<Value>, Option<i64>);

    /// Telegram as tests see it: what was sent and edited, and updates to hand out.
    #[derive(Default)]
    pub struct Fake {
        pub sent: Mutex<Vec<Sent>>,
        pub edited: Mutex<Vec<(i64, i64, String, bool)>>,
        pub notes: Mutex<Vec<String>>,
        next: Mutex<i64>,
    }

    impl Api for Fake {
        fn get_me(&self) -> Result<String, String> {
            Ok("my_agentdeck_bot".into())
        }
        fn get_updates(&self, _offset: i64, _wait: u64) -> Result<Vec<Value>, String> {
            std::thread::sleep(Duration::from_millis(50));
            Ok(Vec::new())
        }
        fn send(
            &self,
            chat: i64,
            text: &str,
            kb: Option<Value>,
            reply: Option<i64>,
        ) -> Result<i64, String> {
            self.sent
                .lock()
                .unwrap()
                .push((chat, text.into(), kb, reply));
            let mut n = self.next.lock().unwrap();
            *n += 1;
            Ok(100 + *n)
        }
        fn edit(&self, chat: i64, msg: i64, text: &str, kb: Option<Value>) -> Result<(), String> {
            self.edited
                .lock()
                .unwrap()
                .push((chat, msg, text.into(), kb.is_some()));
            Ok(())
        }
        fn answer_callback(&self, _id: &str, text: &str) -> Result<(), String> {
            self.notes.lock().unwrap().push(text.into());
            Ok(())
        }
    }

    impl Fake {
        fn last(&self) -> Sent {
            self.sent
                .lock()
                .unwrap()
                .last()
                .cloned()
                .expect("something was sent")
        }
        fn texts(&self) -> Vec<String> {
            self.sent
                .lock()
                .unwrap()
                .iter()
                .map(|s| s.1.clone())
                .collect()
        }
    }

    fn msg(chat: i64, id: i64, text: &str) -> Value {
        json!({ "update_id": id, "message": { "message_id": id, "chat": { "id": chat, "type": "private" }, "from": { "id": chat, "first_name": "Yasin", "username": "yasin" }, "text": text } })
    }

    fn button(chat: i64, data: &str) -> Value {
        json!({ "update_id": 9, "callback_query": { "id": "cb", "data": data, "message": { "message_id": 1, "chat": { "id": chat, "type": "private" } } } })
    }

    /// A one-request-at-a-time stand-in for api.telegram.org: answers each call from `reply`
    /// and keeps what it was asked.
    type Seen = Arc<Mutex<Vec<(String, Value)>>>;

    fn server(reply: fn(&str, &Value) -> Value) -> (String, Seen) {
        use std::io::{BufRead, BufReader, Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let mut r = BufReader::new(c.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let mut len = 0;
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let method = path.rsplit('/').next().unwrap_or("").to_string();
                let out = reply(&method, &body).to_string();
                s2.lock().unwrap().push((path, body));
                let mut c = c;
                let _ = write!(c, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len());
            }
        });
        (base, seen)
    }

    #[test]
    fn the_bot_api_is_called_as_telegram_expects_and_errors_never_show_the_token() {
        let token = "123456789:AAH-secret-token-abcdefghijklmnop";
        let (base, seen) = server(|method, body| match method {
            "getMe" => {
                json!({ "ok": true, "result": { "id": 1, "is_bot": true, "username": "my_agentdeck_bot" } })
            }
            "sendMessage" if body["chat_id"] == json!(7) => {
                json!({ "ok": true, "result": { "message_id": 42 } })
            }
            _ => {
                json!({ "ok": false, "error_code": 400, "description": "Bad Request: chat not found" })
            }
        });
        let mut api = Http::new(token).unwrap();
        api.base = base;
        assert_eq!(api.get_me().unwrap(), "my_agentdeck_bot");
        let kb = json!([[{ "text": "✅ Allow", "callback_data": "abc:a" }]]);
        assert_eq!(api.send(7, "Hi", Some(kb.clone()), Some(5)).unwrap(), 42);
        let err = api.send(8, "Hi", None, None).unwrap_err();
        assert_eq!(err, "Bad Request: chat not found");
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen[0].0, format!("/bot{token}/getMe"));
        let sent = &seen[1].1;
        assert_eq!(sent["chat_id"], json!(7));
        assert_eq!(sent["text"], json!("Hi"));
        assert_eq!(sent["reply_markup"]["inline_keyboard"], kb);
        assert_eq!(sent["reply_parameters"]["message_id"], json!(5));
        // Telegram unreachable: the error has no URL, so no token.
        let mut down = Http::new(token).unwrap();
        down.base = "http://127.0.0.1:9".into();
        let e = down.get_me().unwrap_err();
        assert!(
            e.starts_with("Couldn't reach Telegram") && !e.contains("secret"),
            "{e}"
        );
    }

    #[test]
    fn commands_names_and_addressing() {
        assert_eq!(command("/pair 123456"), ("pair", "123456"));
        assert_eq!(command("/use@my_bot Reviewer"), ("use", "Reviewer"));
        assert_eq!(command("/bots"), ("bots", ""));
        assert_eq!(command("look at /tmp"), ("", "look at /tmp"));
        assert_eq!(command("/tmp/x is big"), ("", "/tmp/x is big"));
        let chunked = chunks(&"line\n".repeat(2000));
        assert!(chunked.len() >= 3 && chunked.iter().all(|c| c.chars().count() <= CHUNK));
        assert_eq!(chunks("short"), vec!["short"]);
        assert_eq!(
            strip_prefix_ci("YAZILIMCI: İstanbul", "Yazılımcı"),
            None,
            "I and ı are different letters"
        );
        assert_eq!(
            strip_prefix_ci("yazılımcı: İstanbul ofisine bak", "Yazılımcı"),
            Some(": İstanbul ofisine bak")
        );
        assert_eq!(
            strip_prefix_ci("REVIEWER: İİİİİİİİİİ", "reviewer"),
            Some(": İİİİİİİİİİ")
        );
        assert_eq!(strip_prefix_ci("Rev", "Reviewer"), None);
        assert_eq!(
            describe("Bash", &json!({ "command": "git push origin main" })),
            "run git push origin main"
        );
        assert_eq!(
            describe("", &json!({ "command": ["rm", "-rf", "build"] })),
            "run rm -rf build"
        );
    }

    mod live {
        use super::*;
        use crate::bots::tests_support::{bot_fields, Setup};

        struct T {
            s: Setup,
            tg: Arc<Telegram>,
            fake: Arc<Fake>,
        }

        fn setup() -> T {
            let s = Setup::new();
            let tg = Arc::new(Telegram::new(
                s.db.clone(),
                s.bots.clone(),
                s.agents.clone(),
                Arc::new(|_| {}),
            ));
            let fake = Arc::new(Fake::default());
            tg.start_with(fake.clone(), Some("my_agentdeck_bot".into()));
            T { s, tg, fake }
        }

        fn pair(t: &T, chat: i64) {
            let code = t.tg.start_pairing().unwrap().pairing_code.unwrap();
            t.tg.handle(&msg(chat, 1, &format!("/pair {code}")));
            assert!(t.tg.paired(chat));
        }

        #[test]
        fn only_a_chat_paired_with_the_code_reaches_the_bots() {
            let t = setup();
            t.s.bots.save("", &bot_fields("Reviewer")).unwrap();
            t.tg.handle(&msg(7, 1, "delete everything"));
            assert!(t.fake.last().1.contains("only answers chats they paired"));
            t.tg.handle(&msg(7, 2, "hello again"));
            assert_eq!(t.fake.sent.lock().unwrap().len(), 1, "told once");
            // A wrong code, then the code expires after too many tries.
            let code = t.tg.start_pairing().unwrap().pairing_code.unwrap();
            for _ in 0..PAIR_TRIES {
                t.tg.handle(&msg(7, 3, "/pair 000000x"));
            }
            t.tg.handle(&msg(7, 4, &format!("/pair {code}")));
            assert!(
                !t.tg.paired(7),
                "the code stopped working after {PAIR_TRIES} wrong tries"
            );
            // Groups never pair.
            let code = t.tg.start_pairing().unwrap().pairing_code.unwrap();
            let mut group = msg(-5, 5, &format!("/pair {code}"));
            group["message"]["chat"]["type"] = json!("group");
            t.tg.handle(&group);
            assert!(!t.tg.paired(-5));
            t.tg.handle(&msg(7, 6, &format!("/pair {code}")));
            assert!(t.tg.paired(7));
            assert_eq!(t.tg.status().chats[0].name, "Yasin (@yasin)");
            assert!(t.tg.status().pairing_code.is_none(), "a code works once");
            assert!(t
                .s
                .bots
                .tasks(&t.s.bots.list().unwrap()[0].id, 10)
                .unwrap()
                .is_empty());
        }

        #[test]
        fn a_message_is_a_task_and_its_answer_comes_back_to_the_chat() {
            let t = setup();
            let rev = t.s.bots.save("", &bot_fields("Reviewer")).unwrap();
            pair(&t, 7);
            // One bot: messages go to it.
            t.tg.handle(&msg(7, 10, "Sum up today's commits"));
            let tasks = t.s.bots.tasks(&rev.id, 10).unwrap();
            assert_eq!(tasks.len(), 1);
            assert_eq!(tasks[0].text, "Sum up today's commits");
            assert_eq!(tasks[0].source, "telegram:7:10");
            assert_eq!(
                t.fake.last().3,
                Some(10),
                "answered as a reply to your message"
            );
            // Two bots: name one, or pick.
            let writer = t.s.bots.save("", &bot_fields("Writer")).unwrap();
            t.tg.handle(&msg(7, 11, "Writer: draft a tweet about the release"));
            assert_eq!(
                t.s.bots.tasks(&writer.id, 10).unwrap()[0].text,
                "draft a tweet about the release"
            );
            t.tg.handle(&msg(7, 12, "@reviewer check PR 3"));
            assert_eq!(t.s.bots.tasks(&rev.id, 10).unwrap().len(), 2);
            t.tg.handle(&msg(7, 13, "what's new?"));
            assert!(t.fake.last().1.starts_with("Which bot is this for?"));
            t.tg.handle(&button(7, &format!("use:{}", writer.id)));
            t.tg.handle(&msg(7, 14, "what's new?"));
            assert_eq!(t.s.bots.tasks(&writer.id, 10).unwrap().len(), 2);

            // Its result comes back as a reply; the same task isn't reported twice.
            let task =
                t.s.bots
                    .tasks(&rev.id, 10)
                    .unwrap()
                    .into_iter()
                    .find(|x| x.text == "Sum up today's commits")
                    .unwrap();
            let done =
                t.s.db
                    .finish_bot_task(&task.id, "done", Some("Three commits: …"), None)
                    .unwrap()
                    .unwrap();
            let before = t.fake.sent.lock().unwrap().len();
            t.tg.on_task(&done);
            t.tg.on_task(&done);
            let sent = t.fake.sent.lock().unwrap().clone();
            assert_eq!(sent.len(), before + 1);
            assert_eq!(sent[before].0, 7);
            assert_eq!(sent[before].1, "✅ Reviewer:\n\nThree commits: …");
            assert_eq!(sent[before].3, Some(10));
        }

        #[test]
        fn scheduled_results_go_to_paired_chats_as_you_chose() {
            let t = setup();
            let b = t.s.bots.save("", &bot_fields("Researcher")).unwrap();
            pair(&t, 7);
            let mine =
                t.s.bots
                    .add_task(&b.id, "Look into X", "you", None)
                    .unwrap();
            let sched =
                t.s.db
                    .add_bot_task(&b.id, "Morning news", "schedule", None, None)
                    .unwrap();
            let before = t.fake.sent.lock().unwrap().len();
            for task in [&mine, &sched] {
                let d =
                    t.s.db
                        .finish_bot_task(&task.id, "done", Some("Notes"), None)
                        .unwrap()
                        .unwrap();
                t.tg.on_task(&d);
            }
            let texts = t.fake.texts();
            assert_eq!(
                texts.len(),
                before + 1,
                "by default only scheduled tasks report"
            );
            assert!(texts
                .last()
                .unwrap()
                .starts_with("✅ Researcher (scheduled: Morning news):"));
            t.tg.set_notify("telegram").unwrap();
            let s2 =
                t.s.db
                    .add_bot_task(&b.id, "Evening news", "schedule", None, None)
                    .unwrap();
            let d =
                t.s.db
                    .finish_bot_task(&s2.id, "done", Some("x"), None)
                    .unwrap()
                    .unwrap();
            t.tg.on_task(&d);
            assert_eq!(t.fake.texts().len(), before + 1);
        }

        #[test]
        fn a_risky_step_is_allowed_or_denied_from_telegram() {
            let t = setup();
            let b = t.s.bots.save("", &bot_fields("Builder")).unwrap();
            pair(&t, 7);
            let sid = t.s.session(&b);
            let req = t.s.db.append_event(&sid, "claude", "control_request/can_use_tool", &json!({ "request_id": "r1", "request": { "tool_name": "Bash", "input": { "command": "git push origin main" } } })).unwrap().unwrap();
            let needs = t
                .s
                .db
                .append_event(
                    &sid,
                    "agentdeck",
                    "bot_needs_you",
                    &json!({ "botId": b.id, "requestId": "r1", "why": "it pushes to a remote" }),
                )
                .unwrap()
                .unwrap();
            let _ = req;
            t.tg.on_event(&needs);
            let (chat, text, kb, _) = t.fake.last();
            assert_eq!(chat, 7);
            assert!(text.contains("Builder needs you"), "{text}");
            assert!(
                text.contains("It wants to: run git push origin main"),
                "{text}"
            );
            assert!(
                text.contains("It waited because it pushes to a remote."),
                "{text}"
            );
            let data = kb.unwrap()[0][0]["callback_data"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(data.len() <= 64 && data.ends_with(":a"));
            // The session isn't running here, so answering fails; the message says it's over.
            t.tg.handle(&button(7, &data));
            assert!(!t.fake.notes.lock().unwrap().is_empty());
            assert!(t
                .fake
                .edited
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.2.contains("No longer waiting") && !e.3));
            // Pressing again: already answered.
            t.tg.handle(&button(7, &data));
            assert_eq!(
                t.fake.notes.lock().unwrap().last().unwrap(),
                "This was answered already"
            );
        }

        #[test]
        fn answered_in_agentdeck_the_telegram_message_says_so() {
            let t = setup();
            let b = t.s.bots.save("", &bot_fields("Builder")).unwrap();
            pair(&t, 7);
            let sid = t.s.session(&b);
            t.s.db.append_event(&sid, "claude", "control_request/can_use_tool", &json!({ "request_id": "r2", "request": { "tool_name": "Bash", "input": { "command": "rm -rf dist" } } })).unwrap();
            let needs =
                t.s.db
                    .append_event(
                        &sid,
                        "agentdeck",
                        "bot_needs_you",
                        &json!({ "botId": b.id, "requestId": "r2", "why": "it deletes files" }),
                    )
                    .unwrap()
                    .unwrap();
            t.tg.on_event(&needs);
            let decided =
                t.s.db
                    .append_event(
                        &sid,
                        "claude",
                        "permission_decision",
                        &json!({ "requestId": "r2", "decision": "deny" }),
                    )
                    .unwrap()
                    .unwrap();
            t.tg.on_event(&decided);
            let edits = t.fake.edited.lock().unwrap().clone();
            assert_eq!(edits.len(), 1);
            assert!(edits[0].2.ends_with("❌ Denied") && !edits[0].3);
        }

        #[test]
        fn a_question_is_asked_with_its_choices() {
            let t = setup();
            let b = t.s.bots.save("", &bot_fields("Planner")).unwrap();
            pair(&t, 7);
            let sid = t.s.session(&b);
            t.s.db.append_event(&sid, "claude", "control_request/can_use_tool", &json!({ "request_id": "q1", "request": { "tool_name": "AskUserQuestion", "input": { "questions": [
                { "question": "Which database?", "header": "DB", "options": [{ "label": "SQLite" }, { "label": "Postgres" }], "multiSelect": false },
                { "question": "Which platforms?", "header": "Targets", "options": [{ "label": "macOS" }, { "label": "Windows" }], "multiSelect": true }
            ] } } })).unwrap();
            let needs =
                t.s.db
                    .append_event(
                        &sid,
                        "agentdeck",
                        "bot_needs_you",
                        &json!({ "botId": b.id, "requestId": "q1", "question": true }),
                    )
                    .unwrap()
                    .unwrap();
            t.tg.on_event(&needs);
            let sent = t.fake.sent.lock().unwrap().clone();
            let (q1, q2) = (&sent[sent.len() - 2], &sent[sent.len() - 1]);
            assert!(q1.1.contains("Which database?") && q2.1.contains("Which platforms?"));
            let opt = |kb: &Option<Value>, row: usize| {
                kb.as_ref().unwrap()[row][0]["callback_data"]
                    .as_str()
                    .unwrap()
                    .to_string()
            };
            // First question: one tap answers it.
            t.tg.handle(&button(7, &opt(&q1.2, 1)));
            assert_eq!(t.fake.notes.lock().unwrap().last().unwrap(), "Got it");
            // Second: pick two, then Done. The session isn't running, so the answer can't go;
            // the note says why.
            t.tg.handle(&button(7, &opt(&q2.2, 0)));
            t.tg.handle(&button(7, &opt(&q2.2, 1)));
            let done = q2.2.as_ref().unwrap()[2][0]["callback_data"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(done.ends_with(":k1"));
            t.tg.handle(&button(7, &done));
            let note = t.fake.notes.lock().unwrap().last().unwrap().clone();
            assert!(note.contains("isn't running"), "{note}");
            let edits = t.fake.edited.lock().unwrap().clone();
            assert!(edits.iter().any(|e| e.2.contains("Your answer: Postgres")));
            assert!(edits
                .iter()
                .any(|e| e.2.contains("Your answer: macOS, Windows")));
        }

        #[test]
        fn a_token_is_checked_before_anything_is_saved() {
            let t = setup();
            for bad in ["", "hello", "abc:defghijklmnopqrstuvwxyz", "123:short"] {
                assert!(t
                    .tg
                    .connect(bad)
                    .unwrap_err()
                    .contains("doesn't look like a bot token"));
            }
            assert_eq!(crate::secrets::get_named(TOKEN), None);
            pair(&t, 7);
            let st = t.tg.disconnect();
            assert!(!st.connected && st.chats.is_empty());
        }
    }
}
