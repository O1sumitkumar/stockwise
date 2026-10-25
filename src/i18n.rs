//! Localized user-facing text: a typed message catalog for English and Hindi.
//!
//! Every message is a `Msg` variant with one string per language, declared
//! together in the `messages!` table below, so a key cannot exist without a
//! translation. Placeholders are positional (`{0}`, `{1}`) and filled with
//! [`tf`]; a test checks both languages use the same ones.
//!
//! Only human-readable output is localized. `--json` / `--csv` output and
//! numbers stay language-neutral; Indian amounts keep lakh/crore grouping in
//! both languages.

use clap::ValueEnum;
use std::fmt::Display;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Lang {
    /// English (default)
    #[value(alias = "english")]
    En,
    /// हिन्दी (Hindi)
    #[value(alias = "hindi")]
    Hi,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Hi => "hi",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Lang::from_str(value.trim(), true).ok()
    }
}

static LANG: OnceLock<(Lang, LangSource)> = OnceLock::new();

/// Set the output language. Called once at startup; later calls are ignored.
pub fn set_lang(lang: Lang, source: LangSource) {
    let _ = LANG.set((lang, source));
}

/// The output language (English until `set_lang` is called).
pub fn lang() -> Lang {
    LANG.get().map_or(Lang::En, |(lang, _)| *lang)
}

/// Where the active language came from.
pub fn lang_source() -> LangSource {
    LANG.get()
        .map_or(LangSource::Default, |(_, source)| *source)
}

/// Where the language came from, for `doctor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LangSource {
    Flag,
    StockwiseLang,
    /// `lang` in the config file.
    Config,
    Locale,
    Default,
}

/// Pick the language before clap parses the command line (the help text is
/// built from it): `--lang` beats `STOCKWISE_LANG`, which beats a Hindi POSIX
/// locale (`LC_ALL`, `LC_MESSAGES`, `LANG` = `hi_IN.UTF-8`). Anything else is
/// English. An invalid `--lang` or `STOCKWISE_LANG` falls through here and is
/// reported by clap.
pub fn detect(args: &[String], env: impl Fn(&str) -> Option<String>) -> (Lang, LangSource) {
    if let Some(lang) = lang_flag(args).as_deref().and_then(Lang::parse) {
        return (lang, LangSource::Flag);
    }
    if let Some(lang) = env("STOCKWISE_LANG").as_deref().and_then(Lang::parse) {
        return (lang, LangSource::StockwiseLang);
    }
    // POSIX precedence: the first non-empty of these decides.
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| env(k))
        .find(|v| !v.is_empty());
    if let Some(locale) = locale {
        let code = locale.split(['_', '.', '@', '-']).next().unwrap_or("");
        if code.eq_ignore_ascii_case("hi") {
            return (Lang::Hi, LangSource::Locale);
        }
    }
    (Lang::En, LangSource::Default)
}

/// [`detect`], with `lang` from the config file between `STOCKWISE_LANG`
/// and the locale: an explicit setting beats the system default.
pub fn detect_with_config(
    args: &[String],
    env: impl Fn(&str) -> Option<String>,
    file: Option<&str>,
) -> (Lang, LangSource) {
    let detected = detect(args, env);
    match (detected.1, file.and_then(Lang::parse)) {
        (LangSource::Locale | LangSource::Default, Some(lang)) => (lang, LangSource::Config),
        _ => detected,
    }
}

/// Whether `value` names a language (`en`, `hi`, ...).
pub fn is_lang(value: &str) -> bool {
    Lang::parse(value).is_some()
}

/// The value of `--lang X` / `--lang=X`, ignoring anything after `--`.
fn lang_flag(args: &[String]) -> Option<String> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--" {
            break;
        }
        if arg == "--lang" {
            return iter.next().cloned();
        }
        if let Some(v) = arg.strip_prefix("--lang=") {
            return Some(v.to_string());
        }
    }
    None
}

/// Text of `msg` in the active language.
pub fn t(msg: Msg) -> &'static str {
    msg.text(lang())
}

/// `t(msg)` with `{0}`, `{1}`, ... replaced by `args`.
pub fn tf(msg: Msg, args: &[&dyn Display]) -> String {
    fill(t(msg), args)
}

/// [`tf`] in a given language — for text that stays English whatever
/// `--lang` says (notifications, `--json`).
pub fn tf_in(lang: Lang, msg: Msg, args: &[&dyn Display]) -> String {
    fill(msg.text(lang), args)
}

fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut out = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &arg.to_string());
    }
    out
}

macro_rules! messages {
    ($($key:ident => $en:literal, $hi:literal;)*) => {
        /// A localized message. See the table in `i18n.rs`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Msg {
            $($key,)*
        }

        impl Msg {
            #[cfg(test)]
            pub const ALL: &'static [Msg] = &[$(Msg::$key,)*];

            pub fn text(self, lang: Lang) -> &'static str {
                match lang {
                    Lang::En => match self { $(Msg::$key => $en,)* },
                    Lang::Hi => match self { $(Msg::$key => $hi,)* },
                }
            }
        }
    };
}

messages! {
    // ── Help ──
    HelpUsage => "Usage:", "उपयोग:";
    HelpOptions => "Options:", "विकल्प:";
    HelpPrintHelp => "Print help for a command", "किसी कमांड की सहायता दिखाएँ";
    GroupStartHere => "Start here", "यहाँ से शुरू करें";
    GroupAnalysis => "Analysis", "विश्लेषण";
    GroupMarket => "Market", "बाज़ार";
    GroupStrategies => "Strategies & bots", "रणनीतियाँ और बॉट";
    GroupFunds => "Funds & F&O", "फंड और F&O";
    GroupPortfolio => "Portfolio", "पोर्टफोलियो";
    GroupInterfaces => "Interfaces", "इंटरफ़ेस";
    GroupLiveTrading => "Live trading", "लाइव ट्रेडिंग";
    GroupSetup => "Setup", "सेटअप";
    AboutQuote => "Real-time quote for one or more stocks", "एक या अधिक शेयरों का ताज़ा भाव";
    AboutDeep => "Everything about a stock in one command", "एक कमांड में किसी शेयर की पूरी जानकारी";
    AboutMarkets => "Major market indices (US + India)", "प्रमुख बाज़ार सूचकांक (अमेरिका + भारत)";
    AboutDashboard => "Markets + portfolio + watchlist + alerts on one screen", "बाज़ार, पोर्टफोलियो, वॉचलिस्ट और अलर्ट एक ही स्क्रीन पर";
    AboutSearch => "Find a ticker by company name", "कंपनी के नाम से टिकर खोजें";
    AboutDoctor => "Check your setup: data directory, network, cache, AI, broker", "अपना सेटअप जाँचें: डेटा फ़ोल्डर, नेटवर्क, कैश, AI, ब्रोकर";
    AboutPortfolio => "Track holdings and P&L (default: show)", "होल्डिंग्स और लाभ/हानि ट्रैक करें (डिफ़ॉल्ट: दिखाएँ)";
    AboutWatch => "Manage your watchlist (default: show)", "अपनी वॉचलिस्ट संभालें (डिफ़ॉल्ट: दिखाएँ)";
    AboutAlert => "Price and technical alerts (default: list)", "भाव और तकनीकी अलर्ट (डिफ़ॉल्ट: सूची)";
    AboutCompletions => "Print a shell completion script (bash, zsh, fish, powershell, elvish)", "शेल कम्प्लीशन स्क्रिप्ट दिखाएँ (bash, zsh, fish, powershell, elvish)";

    // ── Markets ──
    MarketIndia => "India", "भारत";
    MarketUs => "US", "अमेरिका";
    MarketCrypto => "Crypto", "क्रिप्टो";
    MarketFx => "FX", "विदेशी मुद्रा";
    SectionIndianMarkets => "Indian Markets", "भारतीय बाज़ार";
    SectionUsMarkets => "US Markets", "अमेरिकी बाज़ार";
    SectionCrypto => "Crypto (24/7)", "क्रिप्टो (24/7)";
    SectionFx => "Currencies (FX)", "मुद्राएँ (FX)";
    SessionCrypto => "Trades 24/7 — change is since 00:00 UTC", "24/7 ट्रेडिंग — बदलाव 00:00 UTC से";
    SessionFxOpen => "FX trades 24 hours, Monday to Friday", "विदेशी मुद्रा बाज़ार सोमवार से शुक्रवार, 24 घंटे खुला";
    SessionFxWeekend => "FX market closed for the weekend — showing Friday's close", "सप्ताहांत पर विदेशी मुद्रा बाज़ार बंद — शुक्रवार का बंद भाव";

    // ── Quote labels ──
    Price => "Price", "कीमत";
    Change => "Change", "बदलाव";
    Open => "Open", "ओपन";
    DayRange => "Day Range", "दिन की रेंज";
    WeekRange52 => "52-Week Range", "52-सप्ताह रेंज";
    Volume => "Volume", "वॉल्यूम";
    AvgVolume3M => "Avg Volume (3M)", "औसत वॉल्यूम (3 माह)";
    MarketCap => "Market Cap", "मार्केट कैप";
    PeTtm => "P/E (TTM)", "P/E (TTM)";
    EpsTtm => "EPS (TTM)", "EPS (TTM)";
    Ma50 => "50-Day MA", "50-दिन MA";
    Ma200 => "200-Day MA", "200-दिन MA";
    DividendYield => "Dividend Yield", "डिविडेंड यील्ड";
    Unknown => "Unknown", "अज्ञात";
    DataFrom => "Data: {0}", "डेटा: {0}";
    NoQuoteFor => "No quote for {0}{1}", "{0} का भाव नहीं मिला{1}";
    HintNotFoundIn => " (Indian market is the default — for US stocks add `-m us`; use `stockwise search <name>` to find a ticker)", " (भारतीय बाज़ार डिफ़ॉल्ट है — अमेरिकी शेयरों के लिए `-m us` जोड़ें; टिकर खोजने के लिए `stockwise search <नाम>` चलाएँ)";
    HintNotFoundUs => " (use `stockwise search <name>` to find a ticker; Indian stocks need `-m in` or a .NS suffix)", " (टिकर खोजने के लिए `stockwise search <नाम>` चलाएँ; भारतीय शेयरों के लिए `-m in` या .NS जोड़ें)";
    HintNotFoundCrypto => " (use the coin's ticker, e.g. BTC or ETH, or a full pair like BTC-INR)", " (कॉइन का टिकर लिखें, जैसे BTC या ETH, या पूरा जोड़ा जैसे BTC-INR)";
    HintNotFoundFx => " (use a currency pair such as USDINR or EURUSD)", " (मुद्रा जोड़ा लिखें, जैसे USDINR या EURUSD)";

    // ── Search ──
    SearchTitle => "Search: {0}", "खोज: {0}";
    Symbol => "Symbol", "सिंबल";
    Name => "Name", "नाम";
    Type => "Type", "प्रकार";
    Exchange => "Exchange", "एक्सचेंज";
    NoTickersMatch => "No tickers match '{0}'", "'{0}' से कोई टिकर नहीं मिला";

    // ── Dashboard ──
    DashboardTitle => "Dashboard — {0}", "डैशबोर्ड — {0}";
    MarketIndices => "Market Indices", "बाज़ार सूचकांक";
    TopCoins => "Top Coins", "प्रमुख कॉइन";
    MajorPairs => "Major Pairs", "प्रमुख मुद्रा जोड़े";
    PortfolioCount => "Portfolio ({0} holdings)", "पोर्टफोलियो ({0} होल्डिंग्स)";
    WatchlistCount => "Watchlist ({0} symbols)", "वॉचलिस्ट ({0} सिंबल)";
    TotalValue => "Total Value", "कुल मूल्य";
    TotalInvested => "Total Invested", "कुल निवेश";
    PnL => "P&L", "लाभ/हानि";
    PnLPct => "P&L %", "लाभ/हानि %";
    TriggeredAlerts => "Triggered Alerts", "ट्रिगर हुए अलर्ट";
    GetStarted => "Get started", "शुरुआत करें";
    GetStartedTrack => "track a holding (symbol, shares, cost)", "होल्डिंग जोड़ें (सिंबल, शेयर, लागत)";
    GetStartedImport => "or import from your broker", "या अपने ब्रोकर से इम्पोर्ट करें";
    GetStartedWatch => "watch a few stocks", "कुछ शेयरों पर नज़र रखें";
    GetStartedAlert => "get alerted on a price level", "किसी भाव पर अलर्ट पाएँ";

    // ── Portfolio ──
    YourPortfolio => "Your Portfolio", "आपका पोर्टफोलियो";
    Shares => "Shares", "शेयर";
    AvgCost => "Avg Cost", "औसत लागत";
    Value => "Value", "मूल्य";
    Total => "TOTAL", "कुल";
    PortfolioEmpty => "Portfolio is empty. Use 'stockwise portfolio add <SYMBOL> <SHARES> <COST>' to add holdings.", "पोर्टफोलियो खाली है। होल्डिंग जोड़ने के लिए 'stockwise portfolio add <SYMBOL> <SHARES> <COST>' चलाएँ।";

    // ── Indian number units (lakh / crore) ──
    UnitLakhCrore => "L Cr", " लाख करोड़";
    UnitThousandCrore => "K Cr", " हज़ार करोड़";
    UnitCrore => " Cr", " करोड़";
    UnitLakh => "L", " लाख";

    // ── Doctor ──
    DoctorTitle => "StockWise {0} — doctor", "StockWise {0} — डॉक्टर";
    DoctorMarket => "Market", "बाज़ार";
    DoctorMarketFromEnv => "from STOCKWISE_MARKET", "STOCKWISE_MARKET से";
    DoctorMarketDefault => "default; change with -m or STOCKWISE_MARKET", "डिफ़ॉल्ट; -m या STOCKWISE_MARKET से बदलें";
    DoctorLanguage => "Language", "भाषा";
    DoctorLangFlag => "from --lang", "--lang से";
    DoctorLangEnv => "from STOCKWISE_LANG", "STOCKWISE_LANG से";
    DoctorLangConfig => "from the config file", "कॉन्फ़िग फ़ाइल से";
    DoctorLangLocale => "from your locale (LANG)", "आपके लोकेल (LANG) से";
    DoctorLangDefault => "default; --lang hi or STOCKWISE_LANG=hi for Hindi", "डिफ़ॉल्ट; हिन्दी के लिए --lang hi या STOCKWISE_LANG=hi";
    DoctorDataDir => "Data directory", "डेटा फ़ोल्डर";
    DoctorDataFiles => "Data files", "डेटा फ़ाइलें";
    DoctorNoFilesYet => "none yet (created on first use)", "अभी कोई नहीं (पहली बार उपयोग पर बनेंगी)";
    DoctorReachable => "reachable ({0} ms)", "उपलब्ध ({0} ms)";
    DoctorYahooUnreachable => "unreachable — check your internet connection or proxy", "पहुँच से बाहर — अपना इंटरनेट कनेक्शन या प्रॉक्सी जाँचें";
    DoctorDataProvider => "Data provider", "डेटा प्रदाता";
    DoctorProviderDefault => "Yahoo Finance (default; set STOCKWISE_DATA_PROVIDER for another)", "Yahoo Finance (डिफ़ॉल्ट; दूसरे के लिए STOCKWISE_DATA_PROVIDER सेट करें)";
    DoctorProviderAnswered => "{0} answered for {1} ({2} ms); Yahoo fills missing fields", "{1} के लिए {0} ने जवाब दिया ({2} ms); बाकी फ़ील्ड Yahoo से";
    DoctorProviderFellBack => "{0} did not answer ({1}); using Yahoo instead", "{0} से जवाब नहीं मिला ({1}); Yahoo का उपयोग हो रहा है";
    DoctorProviderNoKey => "{0} selected but {1} is not set; using Yahoo", "{0} चुना गया पर {1} सेट नहीं है; Yahoo का उपयोग हो रहा है";
    DoctorProviderUnknown => "unknown provider '{0}' — use yahoo, alphavantage, massive (polygon) or twelvedata", "अज्ञात प्रदाता '{0}' — yahoo, alphavantage, massive (polygon) या twelvedata चुनें";
    DoctorRedis => "Redis cache", "Redis कैश";
    DoctorConnected => "connected", "जुड़ा हुआ";
    DoctorRedisNotUsed => "not used (optional; in-memory cache instead)", "उपयोग में नहीं (वैकल्पिक; मेमोरी कैश चल रहा है)";
    DoctorAi => "AI", "AI";
    DoctorOllamaReady => "Ollama model {0} ready — use `report` or add --ai", "Ollama मॉडल {0} तैयार — `report` चलाएँ या --ai जोड़ें";
    DoctorOllamaNoModel => "Ollama is running, but model {0} is not pulled — run `ollama pull {0}`", "Ollama चल रहा है, पर मॉडल {0} डाउनलोड नहीं है — `ollama pull {0}` चलाएँ";
    DoctorOllamaDown => "Ollama not running (optional; needed for `report` and --ai)", "Ollama नहीं चल रहा (वैकल्पिक; `report` और --ai के लिए ज़रूरी)";
    DoctorCloudReady => "{0}, model {1} — key from {2}; prompts and market data are sent to {0}", "{0}, मॉडल {1} — कुंजी {2} से; प्रॉम्प्ट और मार्केट डेटा {0} को भेजा जाता है";
    DoctorCloudNoKey => "{0} selected but {1} is not set", "{0} चुना गया पर {1} सेट नहीं है";
    DoctorAiUnknown => "unknown provider '{0}' — use ollama, openai, anthropic or gemini", "अज्ञात प्रदाता '{0}' — ollama, openai, anthropic या gemini चुनें";
    DoctorAngel => "Angel One", "Angel One";
    DoctorAngelConfigured => "configured for {0}{1}", "{0} के लिए कॉन्फ़िगर{1}";
    DoctorAngelMaxOrder => ", max order ₹{0}", ", अधिकतम ऑर्डर ₹{0}";
    DoctorAngelNotConfigured => "not configured (optional; `trade setup` for live trading)", "कॉन्फ़िगर नहीं (वैकल्पिक; लाइव ट्रेडिंग के लिए `trade setup`)";
    DoctorKeyring => "Keyring", "कीरिंग";
    DoctorKeyringAvailable => "{0} available for broker secrets", "ब्रोकर सीक्रेट्स के लिए {0} उपलब्ध";
    DoctorKeyringNotUsed => "not used ({0}); broker secrets stay in owner-only files", "उपयोग में नहीं ({0}); ब्रोकर सीक्रेट्स केवल आपकी पढ़ी जा सकने वाली फ़ाइलों में रहते हैं";
    DoctorBrokerPlaceholder => "placeholder values in {0} — `trade config --broker {1}`", "{0} में अभी नमूना मान हैं — `trade config --broker {1}`";
    DoctorBrokerOrdersOn => "live orders enabled", "लाइव ऑर्डर चालू";
    DoctorBrokerOrdersOff => "live orders off (read-only)", "लाइव ऑर्डर बंद (केवल देखना)";
    DoctorAngelStopGttOn => "SL/SL-M and GTT enabled", "SL/SL-M और GTT चालू";
    DoctorMarketDataDown => "Market data is unreachable", "मार्केट डेटा तक पहुँच नहीं हो पा रही";

    // ── Alerts ──
    AlertsTitle => "Alerts", "अलर्ट";
    AlertAbove => "above {0}", "{0} से ऊपर";
    AlertBelow => "below {0}", "{0} से नीचे";
    AlertRsiBelow => "RSI({0}) below {1}", "RSI({0}) {1} से नीचे";
    AlertRsiAbove => "RSI({0}) above {1}", "RSI({0}) {1} से ऊपर";
    AlertCrossAbove => "close crosses above the {0}-day {1}", "क्लोज़ {0}-दिन के {1} को ऊपर की ओर पार करे";
    AlertCrossBelow => "close crosses below the {0}-day {1}", "क्लोज़ {0}-दिन के {1} को नीचे की ओर पार करे";
    AlertBreakoutHigh => "close above the previous {0}-day high", "क्लोज़ पिछले {0} दिनों के उच्चतम से ऊपर";
    AlertBreakdownLow => "close below the previous {0}-day low", "क्लोज़ पिछले {0} दिनों के न्यूनतम से नीचे";
    AlertChangeAbove => "day change of {0} or better", "दिन का बदलाव {0} या उससे अधिक";
    AlertChangeBelow => "day change of {0} or worse", "दिन का बदलाव {0} या उससे कम";
    AlertVolumeSpike => "volume {0}× its {1}-day average", "वॉल्यूम {1}-दिन के औसत का {0}×";
    AlertResultsIn => "results due within {0} days", "{0} दिनों के भीतर नतीजे";

    // ── Calendar ──
    CalendarTitle => "Results & corporate calendar — next {0} days", "नतीजे और कॉर्पोरेट कैलेंडर — अगले {0} दिन";
    CalDate => "Date", "तारीख";
    CalWhen => "When", "कब";
    CalEvent => "Event", "इवेंट";
    CalDetails => "Details", "विवरण";
    CalSource => "Source", "स्रोत";
    CalToday => "today", "आज";
    CalTomorrow => "tomorrow", "कल";
    CalInDays => "in {0} days", "{0} दिन में";
    CalEstimated => "estimated", "अनुमानित";
    CalThisWeek => "{0} event(s) in the next 7 days", "अगले 7 दिनों में {0} इवेंट";
    CalNoEvents => "No results, dividends or splits in the next {0} days for {1} symbol(s).", "अगले {0} दिनों में {1} सिंबल के लिए कोई नतीजे, डिविडेंड या स्प्लिट नहीं।";
    CalNoSymbols => "Nothing to look up — add holdings or watchlist symbols, or name them: `stockwise calendar TCS INFY`.", "देखने के लिए कुछ नहीं — होल्डिंग या वॉचलिस्ट में सिंबल जोड़ें, या नाम दें: `stockwise calendar TCS INFY`।";
    CalSources => "Source: Yahoo Finance (results, ex-dividend and payment dates).", "स्रोत: Yahoo Finance (नतीजे, एक्स-डिविडेंड और भुगतान की तारीखें)।";
    CalSourcesNse => "Indian stocks: also NSE board meetings and corporate actions (filed results dates, dividends, splits, bonus issues).", "भारतीय शेयर: NSE बोर्ड बैठकें और कॉर्पोरेट एक्शन भी (दाखिल नतीजों की तारीखें, डिविडेंड, स्प्लिट, बोनस)।";
    EventResults => "Results", "नतीजे";
    EventBoardMeeting => "Board meeting", "बोर्ड बैठक";
    EventExDividend => "Ex-dividend", "एक्स-डिविडेंड";
    EventDividendPaid => "Dividend paid", "डिविडेंड भुगतान";
    EventSplit => "Split", "स्प्लिट";
    EventBonus => "Bonus", "बोनस";
    EventRights => "Rights issue", "राइट्स इश्यू";
    EventOther => "Corporate action", "कॉर्पोरेट एक्शन";

    // ── Web dashboard (`serve`) ──
    ServeDashboard => "Dashboard", "डैशबोर्ड";
    ServeOpenFailed => "Could not open a browser ({0}); open {1} yourself", "ब्राउज़र नहीं खुल सका ({0}); {1} खुद खोलें";

    // ── Errors ──
    ErrorPrefix => "Error:", "त्रुटि:";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flag_beats_env_beats_locale() {
        let env = env_from(&[("STOCKWISE_LANG", "en"), ("LANG", "hi_IN.UTF-8")]);
        assert_eq!(
            detect(&args(&["stockwise", "--lang", "hi", "quote", "TCS"]), &env),
            (Lang::Hi, LangSource::Flag)
        );
        assert_eq!(
            detect(&args(&["stockwise", "quote", "--lang=hi"]), &env),
            (Lang::Hi, LangSource::Flag)
        );
        assert_eq!(
            detect(&args(&["stockwise", "quote"]), &env),
            (Lang::En, LangSource::StockwiseLang)
        );
        let env = env_from(&[("LANG", "hi_IN.UTF-8")]);
        assert_eq!(
            detect(&args(&["stockwise"]), &env),
            (Lang::Hi, LangSource::Locale)
        );
    }

    #[test]
    fn config_file_beats_locale_but_not_env_or_flag() {
        let locale = env_from(&[("LANG", "en_US.UTF-8")]);
        assert_eq!(
            detect_with_config(&args(&["stockwise"]), &locale, Some("hi")),
            (Lang::Hi, LangSource::Config)
        );
        let env = env_from(&[("STOCKWISE_LANG", "en")]);
        assert_eq!(
            detect_with_config(&args(&["stockwise"]), &env, Some("hi")),
            (Lang::En, LangSource::StockwiseLang)
        );
        assert_eq!(
            detect_with_config(&args(&["stockwise", "--lang", "en"]), &locale, Some("hi")),
            (Lang::En, LangSource::Flag)
        );
        assert_eq!(
            detect_with_config(&args(&["stockwise"]), &locale, None),
            (Lang::En, LangSource::Default)
        );
    }

    #[test]
    fn locale_detection_follows_posix_precedence() {
        let env = env_from(&[("LC_ALL", "en_US.UTF-8"), ("LANG", "hi_IN.UTF-8")]);
        assert_eq!(detect(&args(&["stockwise"]), &env).0, Lang::En);
        let env = env_from(&[("LC_ALL", ""), ("LC_MESSAGES", "hi_IN"), ("LANG", "C")]);
        assert_eq!(detect(&args(&["stockwise"]), &env).0, Lang::Hi);
        let env = env_from(&[("LANG", "C.UTF-8")]);
        assert_eq!(
            detect(&args(&["stockwise"]), &env),
            (Lang::En, LangSource::Default)
        );
        // `hi` must be the language code, not a substring of another one.
        let env = env_from(&[("LANG", "chi_CN.UTF-8")]);
        assert_eq!(detect(&args(&["stockwise"]), &env).0, Lang::En);
    }

    #[test]
    fn invalid_values_fall_through_and_args_after_double_dash_are_ignored() {
        let env = env_from(&[("STOCKWISE_LANG", "klingon")]);
        assert_eq!(
            detect(&args(&["stockwise", "--lang", "xx"]), &env),
            (Lang::En, LangSource::Default)
        );
        assert_eq!(
            detect(&args(&["stockwise", "--", "--lang", "hi"]), env_from(&[])).0,
            Lang::En
        );
        assert_eq!(
            detect(&args(&["stockwise", "--lang", "Hindi"]), env_from(&[])).0,
            Lang::Hi
        );
    }

    #[test]
    fn every_message_has_both_languages_with_matching_placeholders() {
        let placeholders = |s: &str| -> Vec<String> {
            let mut found: Vec<String> = (0..10)
                .map(|i| format!("{{{i}}}"))
                .filter(|p| s.contains(p.as_str()))
                .collect();
            found.sort();
            found
        };
        for msg in Msg::ALL {
            let en = msg.text(Lang::En);
            let hi = msg.text(Lang::Hi);
            assert!(!en.is_empty() && !hi.is_empty(), "{msg:?} is empty");
            assert_eq!(
                placeholders(en),
                placeholders(hi),
                "{msg:?} placeholders differ"
            );
        }
    }

    #[test]
    fn hindi_strings_are_actually_hindi() {
        // Labels that are the same in both languages are fine (P/E, AI); the
        // catalog as a whole must not be English copied over.
        let translated = Msg::ALL
            .iter()
            .filter(|m| m.text(Lang::Hi) != m.text(Lang::En))
            .count();
        assert!(
            translated * 10 >= Msg::ALL.len() * 9,
            "{translated}/{}",
            Msg::ALL.len()
        );
    }

    #[test]
    fn fill_replaces_positional_placeholders() {
        assert_eq!(fill("{0} of {1}, {0}", &[&"a", &2]), "a of 2, a");
        assert_eq!(
            Msg::PortfolioCount.text(Lang::Hi).replace("{0}", "3"),
            "पोर्टफोलियो (3 होल्डिंग्स)"
        );
    }
}
