<div align="center">

[English](../../README.md) · **Türkçe** · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · [Français](README.fr.md) · [Português](README.pt.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [العربية](README.ar.md)

_Bu bir çeviridir; esas metin [İngilizce README](../../README.md)'dir._

# AgentDeck

**Zaten kullandığınız tüm yapay zekâ kodlama ajanları için tek bir Mac çalışma alanı.**

Claude Code, Codex, Gemini CLI, Kimi Code ve kendi modellerinizi yan yana çalıştırın. Yaptıkları işi temiz ve düzenli bir çıktı olarak okuyun, her diff'i inceleyin, sorularını ve onay isteklerini tek bir yerden yanıtlayın; planlamayı, geliştirmeyi ve birbirlerinin işini incelemeyi de onlara bırakın.

Sizin CLI'larınız. Sizin hesaplarınız. API anahtarı gerekmez.

[Mac için indir](https://github.com/kalismeras61/agentdeck/releases/latest) · [Yenilikler](https://github.com/kalismeras61/agentdeck/releases) · [Nasıl çalışır](#nasıl-çalışır) · [Kaynaktan derleme](#kaynaktan-derleme)

![Lisans: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-8b5cf6)
![macOS 12+](https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%26%20Intel-0B0D14)
![Tauri 2 ile geliştirildi](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-24C8DB)

![AgentDeck: komut çalıştırmak için izin isteyen bir Claude Code oturumu, sağda yaptığı değişikliklerin diff'i](../screenshots/readme/workspace.png)

</div>

## Neden AgentDeck

Kodlama ajanları terminalde harika iş çıkarır; ta ki beş projede üçünü birden çalıştırana kadar. Onay istekleri ekranın yukarısında kaybolur, diff'ler logların arasına gömülür ve bir ajanın işini diğerine kontrol ettirmek kopyala-yapıştır demektir.

AgentDeck ajanları olduğu gibi bırakır ve onlara gerçek bir çalışma alanı sunar:

- **Gerçek CLI'ları çalıştırır.** `claude`, `codex`, `gemini` sizin kurulumunuzdan; Terminal'dekiyle aynı PATH, yapılandırma ve girişle başlar. Terminalinizde çalışıyorsa burada da çalışır.
- **Kimlik bilgilerinize asla dokunmaz.** Her ajan kendi giriş akışıyla oturum açar. AgentDeck token okumaz, saklamaz ya da göndermez; kendi modelleriniz için eklediğiniz API anahtarları macOS Anahtar Zinciri'nde kalır.
- **Her şey Mac'inizde kalır.** Oturumlar yerel bir SQLite veritabanında tutulur. Hesap yok, telemetri yok, sunucu yok.

## Neler yapabilirsiniz

### Her projede, istediğiniz ajanla çalışın

Her proje bir klasördür; her oturum, o klasörde çalışan bir ajandır. İlk mesajdan önce ajanı değiştirin, her ajanın sunduğu modeller arasından seçim yapın, konuşmanın ortasında model değiştirin. Proje olmadan sohbet başlatın, oturumları projeler arasında taşıyın, yeniden adlandırın ya da Claude Code, Codex ve Gemini CLI'ın Mac'inizde zaten kaydettiği oturumları içe aktarın; konuşma kaldığı yerden devam eder.

Ajanın ne kadar derin düşüneceğini de siz seçersiniz. Claude Code ve Codex'te modelin yanında bir **efor** ayarı bulunur (düşükten azamiye kadar, yalnızca o modelin desteklediği seviyeler). Menü, ajanın o anda kullandığı eforu gösterir; Claude Code çalışırken yaptığınız değişiklik hemen uygulanır, yeni oturumların hangi eforla başlayacağını ise Ayarlar'dan belirlersiniz.

### Değişiklikleri bir pull request inceler gibi inceleyin

Ajanın dokunduğu her dosya, dosya dosya yan yana ya da birleşik bir diff olarak görünür; klavyeyle gezinebilir, dosyaları "görüldü" olarak işaretleyebilirsiniz. Sizin yerinize hiçbir şey commit ya da push edilmez.

Ajanın aldığı ya da baktığı ekran görüntüleri ve görseller (bir simülatör ekran görüntüsü, render edilmiş bir sayfa, okuduğu bir görsel) doğrudan konuşmada ve Önizleme sekmesinde görünür; tam boyutta görmek için üzerine tıklamanız yeterli.

![Bir ajanın değişikliklerini yan yana incelemek](../screenshots/readme/review.png)

### Onaylayın, reddedin ya da sormasına izin verin

Onayınızı gerektiren komutlar ve düzenlemeler, neyin nerede çalışacağını, neyi etkileyeceğini ve ne kadar riskli olduğunu gösteren kartlar olarak karşınıza çıkar. Plan modu, ajanın hiçbir şeyi değiştirmeden önce okuyup öneri sunmasını sağlar. Her ajanın ayrıca, mesaj kutusunda ve Ayarlar'da seçilen bir onay seviyesi vardır: her seferinde sor, dosya düzenlemelerinin ilerlemesine izin ver, Auto (sıradan düzenlemeler ve komutlar ilerler, riskli olanlar ve proje dışındaki her şey yine sizi bekler) ya da her şey. Ajanın bir sorusu olduğunda (Claude Code'un AskUserQuestion'ı, Codex'in request_user_input'u) gerçek bir form görürsünüz: bir seçenek seçin, birkaçını işaretleyin ya da kendi yanıtınızı yazın.

![İki soru soran bir ajan: biri tek seçimli, diğeri çok seçimli](../screenshots/readme/question.png)

### Ajanları birlikte çalıştırın

- **İş akışları: plan → geliştirme → inceleme.** Her aşamaya istediğiniz ajanı atayın; örneğin Claude Code planlasın, Codex geliştirsin, Claude Code incelesin. Her aşama biter bitmez işi bir sonrakine devreder, inceleme notları doğrudan geliştiren ajana geri gider ve inceleyen ajan APPROVED yazdığında akış durur. Sıradan istekler ajanlar adına onaylanır; riskli olanlar (silme, push, deploy, kimlik bilgileri, proje dışındaki her şey) yine sizi bekler.
- **Devret.** Herhangi bir planı ya da yanıtı tek tıkla başka bir ajana gönderin.
- **Eşli çalışma.** İki ajan, biri DONE diyene kadar birbiriyle konuşur.

![Plan, geliştirme ve inceleme iş akışı: Codex geliştirdi, Claude Code inceleyip onayladı](../screenshots/readme/workflow.png)

### Çalışmaya devam eden botlar

Bot, kalıcı bir ajandır. Ona bir ad, bir iş, talimatlar ve bir ajan ile model (Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code, Antigravity ya da kendi modelleriniz) verin, ardından görevler atayın. Siz başka bir işle uğraşırken bot bu görevleri arka planda tamamlar.

- **Kendine ait bir yer.** Her botun kendi klasörü (`~/AgentDeck/Bots/<name>`) vardır ve bir görev bir proje belirtmedikçe orada çalışır. Herhangi bir görevi herhangi bir projeye yönlendirebilirsiniz; bot her yer için ayrı bir konuşma tutar, böylece nerede ne yaptığını hatırlar.
- **Görev sırası.** Bot meşgulken de görev ekleyebilirsiniz; görevleri tek tek çalıştırır. Duraklatın, sürdürün, iptal edin ya da yeniden deneyin; tamamlanan her görev, botun raporunu sonuç olarak saklar.
- **Hafıza.** Her bot, klasöründe kısa bir hafıza dosyası tutar. Her görevin başında bu dosyayı okur ve hatırlamaya değer olanları not eder: kurallar, kararlar, nelerin nerede olduğu, neleri tercih ettiğiniz. Dosyayı botun sayfasında okuyup düzenleyebilirsiniz.
- **Zamanlamalar.** "Hafta içi 09:00'da: dün birleştirilen pull request'leri incele", "6 saatte bir: kararsız testleri bul". Her çalıştırma, her görev gibi sıraya girer.
- **Yönetici bot.** Botlar, AgentDeck'in kendi araçlarıyla (`list_bots`, `delegate_task`, `wait_for_tasks`) bir görevin parçalarını başka botlara devredip sonuçlarını bekleyebilir. Lider bir bot, işi bir inceleyici, bir test uzmanı ve bir yazar arasında bölüp raporlarını bir araya getirebilir.
- **Modeller arası orkestrasyon.** Bir model yönetsin, diğerleri işi yapsın: en güçlü modelinizdeki bir yönetici planlar ve kontrol eder, Opus, Sonnet, Codex ya da Gemini üzerindeki geliştirici botlar kodu yazar. Bir bota birden fazla model seçeneği verdiğinizde, yöneticisi her görev için seçim yapar (basit işe hızlı bir model, zor işe daha güçlü biri, bir de ne kadar efor harcanacağı). "Her görev kendi worktree'sinde" seçeneğini açtığınızda her görev yeni bir dalda çalışır; böylece birkaç geliştirici bot birbirinin işini bozmadan aynı depoda aynı anda çalışabilir. Her bot, görevlerinin kullanımını (görevler, token'lar, API fiyatıyla maliyet) ajanının plan limitlerinin yanında gösterir.
- **Gözetimsiz ama denetimsiz değil.** Bot bir görev üzerinde çalışırken, kendi klasörü ve görevin projesi içindeki sıradan istekler onun adına onaylanır. Riskli her şey (silme, push, deploy, kimlik bilgileri, başka bir yerdeki her şey) ve her soru sizi bekler; bot da kenar çubuğunda "Sizi bekliyor" olarak görünür.
- **Telegram.** Kendi Telegram botunuzu bağlayın (@BotFather'dan alınan bir token; Anahtar Zinciri'nizde saklanır) ve sohbetinizi bir kodla eşleştirin. Ardından telefonunuzdan bir görev yazın; botun yanıtı oraya geri gelir. Belirli bir botu seçmek için mesaja botun adıyla başlayın ("Reviewer: …"). Bir botun size ihtiyacı olduğunda riskli adım, soru ya da plan düğmelerle gelir: izin ver, reddet, bir yanıt seç. AgentDeck yeni mesajları Telegram'dan kendisi çeker; böylece Mac'inizdeki hiçbir şey internete açılmaz ve yalnızca eşleştirdiğiniz sohbetlere yanıt verilir.

- **Bağlantılar.** Botlar kod yazmaktan fazlasını yapabilir. Ayarlar → Bağlantılar bölümünden bir tarayıcı (Playwright), GitHub, Notion, Mac'inizdeki Takvim, Anımsatıcılar, Mail ve Notlar (bu uygulamalar ve içlerinde zaten bulunan hesaplarınız üzerinden) ya da komutu veya adresiyle herhangi bir MCP sunucusu ekleyin; her biri kendi hesabınızla çalışır ve token'lar Anahtar Zinciri'nizde kalır. Ardından her bot için neleri kullanabileceğini seçin. Bot bir görev üzerinde çalışırken bir bağlantının okuduğu şeylere izin verilir; değiştirdiği şeyler (birleştirilen bir pull request, eklenen bir etkinlik, açılan bir e-posta taslağı) ise o bağlantının kendi başına hareket etmesine izin vermediğiniz sürece sizi bekler. Botlar hiçbir zaman e-posta göndermez.
- **Tetikleyiciler.** Zamanlamalar ve Telegram'ın yanı sıra bot şu durumlarda da iş alır: bir web sayfası değiştiğinde (bir fiyat, bir duyuru), bir klasöre yeni dosyalar geldiğinde, deponuzdaki bir issue ya da pull request `agentdeck` etiketini aldığında veya Mac'inizdeki bir betik botun hook'unu çağırdığında (`curl -X POST http://127.0.0.1:47810/hooks/…`). Botlar ayrıca `notify_me` ile size bir şeyi hemen, Telegram'da ve bildirim olarak iletebilir.
- **Hazır botlar.** Araştırmacı, Gelen kutusu asistanı, Gün planlayıcı, Sosyal medya yazarı, Gözcü ya da Geliştirici ile başlayın ve istediğiniz her şeyi değiştirin.

![Sırasındaki görevleri tamamlayan bir bot; zamanlamaları ve hafızası bir sekme ötede](../screenshots/readme/bots.png)

### İşi git worktree'lerinde izole edin

Bir oturumu kendi worktree'si ve dalında başlatın; böylece ajan çalışma kopyanıza karışamaz. Git görünümünde dal grafiğini izleyin, ardından worktree'yi geri birleştirin (iki taraf da aynı satırları değiştirdiyse açık bir uyarıyla) ya da tamamen silin.

![Bir ajanın worktree'sini master dalına geri birleştirmek](../screenshots/readme/worktree.png)

### Tüm komutlar parmaklarınızın ucunda

Mesaj kutusuna `/` yazdığınızda, o klasörde ajanın kendi komutları (Claude Code'un yerleşik komutları, özel komutlarınız ve becerileriniz, Gemini CLI'ın komutları, Codex prompt'larınız) ile AgentDeck'in komutları (`/plan`, `/terminal`, `/workflow`, `/git` …) listelenir.

![Slash komutları menüsü](../screenshots/readme/slash.png)

### Ve dahası

- **Ajan kurulumu.** `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, beceriler, komutlar ve alt ajanları hem global hem de proje bazında, tek bir ağaçtan düzenleyin.
- **Akıllı terminal.** Proje klasöründe kendi shell'iniz. Seçili metin ya da son 80 satır hakkında herhangi bir ajana soru sorun; tek tıkla "Bu hatayı açıkla" deyin.
- **Her şeyi ekleyin.** Dosya ya da klasör sürükleyip bırakın, ekran görüntüsü yapıştırın veya dosya seçin. Görseller ajana görsel olarak ulaşır.
- **Kendi modelleriniz.** DeepSeek, Kimi, GLM (Z.ai), Ollama, LM Studio, OpenRouter ya da OpenAI veya Anthropic uyumlu herhangi bir uç nokta; aynı onaylar, plan modu ve iş akışlarıyla.
- **Plan limitleri bir bakışta.** Claude Code ve Codex oturumlarında mesaj kutusunun altında, 5 saatlik ve haftalık limitlerinizin ne kadarını kullandığınız ve ne zaman sıfırlanacakları görünür.
- **Takıldığı yerde giriş yapın.** Bir ajanın girişe ihtiyacı olduğunda, bir düğme ajanın kendi giriş akışını yerleşik bir terminalde çalıştırır ve mesajınızı yeniden gönderir.
- **Kendini günceller.** Yeni sürümler imzalı ve Apple tarafından onaylıdır (notarized); her güncelleme, kurulmadan önce uygulamaya gömülü bir anahtarla doğrulanır.

### Kendi dilinizde

Arayüzün tamamı **English, Türkçe, Deutsch, Русский, Español, Français, Português, 简体中文, 日本語, 한국어 ve العربية** dillerinde kullanılabilir; Arapça için sağdan sola düzen de desteklenir. AgentDeck Mac'inizin dilini takip eder; isterseniz Ayarlar → Görünüş bölümünden başka bir dil seçebilirsiniz. Ajan yanıtları, kod, komutlar ve dosya adları asla çevrilmez; ajanlar onlara hangi dilde yazarsanız o dilde yanıt verir.

![Türkçe ve sağdan sola Arapça arayüzle AgentDeck](../screenshots/readme/languages.png)

## Mac'inizi yormaz

AgentDeck yerel (native) bir Tauri uygulamasıdır: küçük bir Rust çekirdeği ve macOS'un kendi WebKit'i; içine paketlenmiş bir tarayıcı motoru yoktur.

| | Bir Mac'te ölçüldü (release derlemesi, Etkinlik Monitörü) |
|---|---|
| İndirme | 22 MB disk görüntüsü (Apple Silicon + Intel tek uygulamada) |
| Kurulu boyut | 50 MB |
| Bellek, yalnızca AgentDeck | ~31 MB uygulama + ~207 MB arayüz + ~18 MB sistem yardımcıları ≈ **256 MB** |
| Boştayken CPU | **%0,1**, boştayken uyanma yok |
| Açılış | Arayüz 446 KB'lık ana paketi yükler; inceleme, terminal, galeri ve ayarlar açıldıklarında yüklenir |

Ajanlar ayrı süreçler olarak çalışır ve kendi belleklerini kullanır; çalışırken her biri kabaca 140–210 MB tüketir (Linux'ta ölçüldü: Claude Code ~188 MB, Codex ~159 MB). Bunu kontrol altında tutmak için 10 dakika boşta kalan ajan kapatılır (yeniden yazdığınızda konuşması kaldığı yerden devam eder), aynı anda en fazla 6 ajan açık kalır ve uzun geçmişler 80'er tur halinde yüklenir.

## Desteklenen ajanlar

| Ajan | AgentDeck onunla nasıl konuşur | Giriş |
|---|---|---|
| **Claude Code** | stream-json ve izin istemi aracıyla `claude` | Claude Pro, Max, Team ya da Enterprise veya Anthropic Console |
| **Codex** | `codex app-server`, stdio üzerinden JSON-RPC | ChatGPT planı ya da OpenAI API anahtarı |
| **Gemini CLI** | Agent Client Protocol | Google Workspace ya da Cloud hesabı veya bir Gemini API anahtarı |
| **Grok Build** | Agent Client Protocol | xAI hesabı ya da API anahtarı |
| **Kimi Code** | `kimi acp`, Agent Client Protocol | Kimi Code planı ya da Moonshot AI API anahtarı |
| **Antigravity** *(deneysel, varsayılan olarak kapalı)* | `agy` headless stream-json | Google hesabı; Google'ın koşullarıyla ilgili Ayarlar'daki nota bakın |
| **Kendi modelleriniz** | AgentDeck'in yerleşik ajanı ya da Anthropic uyumlu API'ler için Claude Code içinden | Anahtar Zinciri'nizdeki API anahtarı |

En az birini kurun. İlk açılış ekranı hangilerinin Mac'inizde kurulu olduğunu gösterir; geri kalanları da her ajanın resmi komutuyla, ne yaptığını görebileceğiniz Terminal'de kurar ya da onlara giriş yapar.

## Kurulum

1. En son `AgentDeck-<version>.dmg` dosyasını [Sürümler](https://github.com/kalismeras61/agentdeck/releases/latest) sayfasından indirin.
2. AgentDeck'i Uygulamalar klasörüne sürükleyip açın. Uygulama imzalı ve Apple onaylı olduğu için çift tıklamayla açılır.
3. Bir ajan kurmak için **Başlayın** adımlarını izleyin, ardından bir proje klasörü açın.

macOS 12 veya üzeri, Apple Silicon ya da Intel.

## Nasıl çalışır

```
┌──────────────────────── AgentDeck (Tauri app) ────────────────────────┐
│  React UI: conversation, diffs, approvals, questions, workflows       │
│                │  validated, provider-neutral events (Zod)            │
│  Rust core: sessions · SQLite · git · PTY · workflow engine           │
└──────┬──────────────┬──────────────┬──────────────┬───────────────────┘
       │ stream-json  │ JSON-RPC     │ ACP          │ OpenAI / Anthropic API
       ▼              ▼              ▼              ▼
   claude          codex app-server  gemini, grok,  your models
                                     kimi
   (your install, your sign-in, your folder)
```

- Her ajan, oturumun klasöründe ve login shell'inizin ortamıyla bir alt süreç olarak çalışır.
- Ajanların çıktısı önce olduğu gibi saklanır, ardından tek bir olay akışına (mesajlar, araç çağrıları, diff'ler, onaylar, sorular, planlar) dönüştürülür ve ekranda herhangi bir şey gösterilmeden önce doğrulanır. Hatalı biçimlendirilmiş her şey anlaşılır bir hata olarak gösterilir, asla çalıştırılmaz.
- Onay isteklerini siz, sıradan istekler için bir iş akışı ya da bot veya bir ajan için seçtiğiniz onay seviyesi yanıtlar. Bir ajanı "Her şey" seviyesine almadıkça tehlikeli hiçbir şey sizin yerinize onaylanmaz.
- Ön yüzün shell erişimi yoktur: yalnızca AgentDeck'in kendi, kapsamı dar tutulmuş komutlarını çağırabilir.

## Kaynaktan derleme

Gereksinimler: Node 22+, Rust 1.77+ ve Xcode komut satırı araçları.

```bash
npm install

npm run dev                 # web preview with labeled demo data, http://localhost:1420
npm run tauri dev           # desktop app

# Checks
npm run typecheck && npm run lint && npm test
(cd src-tauri && cargo fmt --check && cargo clippy && cargo test)

# End to end with the real CLIs against local stand-in APIs (no accounts needed)
python3 scripts/mock_anthropic.py 8787 & python3 scripts/mock_openai.py 8788 & python3 scripts/mock_gemini.py 8789 &
(cd src-tauri && AGENTDECK_IT=1 cargo test -- --ignored --test-threads=1)

npm run tauri build         # .app / .dmg
```

Sürüm yayınlama süreci (imzalı, Apple onaylı, universal, kendini güncelleyen) [docs/RELEASING.md](../RELEASING.md) dosyasında; nelerin hazır olduğu ve her parçanın nasıl doğrulandığı [docs/STATUS.md](../STATUS.md) dosyasında; ajan protokolleri ise [docs/PROVIDERS.md](../PROVIDERS.md) dosyasında anlatılıyor.

<details>
<summary><b>Klavye kısayolları</b></summary>

| İşlem | Tuşlar |
|---|---|
| Yeni oturum | ⌘N |
| Oturumlarda ara | ⌘K |
| Kenar çubuğunu / bağlam panelini aç-kapat | ⌘B / ⌘. |
| Bağlam sekmeleri | ⌘1 … ⌘4 |
| Önceki / sonraki oturum | ⌥↑ / ⌥↓ |
| Mesaj kutusuna odaklan | ⌘L |
| Komutlar | mesajın başında / |
| Zengin / Terminal görünümü | ⌘⇧R / ⌘⇧T |
| Tüm oturumlar / İnceleme / İzinler / Git / Ajan kurulumu / Galeri | ⌘⇧S / ⌘⇧D / ⌘⇧A / ⌘⇧H / ⌘⇧E / ⌘⇧G |
| Ayarlar | ⌘, |
| Oturumu yeniden adlandır | başlığa çift tıklayın ya da kenar çubuğunda F2'ye basın |
| İnceleme: sonraki/önceki dosya, değişiklik, görüldü olarak işaretle | J / K, N / P, V |
| Gönder / yeni satır | ↵ / ⇧↵ |

İzin isteklerinin bilinçli olarak bir kısayolu yoktur.

</details>

<details>
<summary><b>Proje yapısı</b></summary>

```
src/                     React app
  lib/events.ts          normalized event schema and validation
  lib/claude.ts, codex.ts, acp.ts, api.ts, normalize.ts   agent logs → events
  components/            sidebar, conversation, composer, terminal, panels
  components/blocks/     renderers: markdown, code, diff, tools, approvals, questions, plans, workflows
  views/                 review, permissions, git, agent setup, settings, get started
  locales/               interface translations (English text is the key)
src-tauri/src/           Rust core
  claude.rs, codex.rs, acp.rs, agy.rs, api.rs   one manager per agent protocol
  flow.rs, relay.rs      workflows and relays
  git.rs, pty.rs, db.rs, import.rs, context.rs, commands.rs, updates.rs
scripts/                 stand-in APIs for tests, release scripts
docs/                    status, providers, releasing, screenshots
```

Teknoloji yığını: Tauri 2 (Rust), React 19, TypeScript (strict), Vite, Tailwind CSS 4, Radix UI, Zustand, Zod, Shiki, Monaco, xterm.js, SQLite.

</details>

## Katkıda bulunma

Issue'lar ve pull request'ler memnuniyetle karşılanır. PR açmadan önce yukarıdaki kontrolleri çalıştırın; arayüz metinlerini kısa ve sade tutun.

**Çeviriler.** Arayüz çevirileri yapay zekâ yardımıyla hazırlandı ve testlerle kontrol edildi, ancak henüz anadili bu dil olan kişilerce gözden geçirilmedi; bu yüzden düzeltmeleriniz çok değerli: `src/locales/<language>.json` dosyasını düzenleyin ve `node scripts/i18n-validate.mjs` komutunu çalıştırın. Yeni arayüz metinleri `t()` üzerinden geçer; ayrıntılar için [docs/I18N.md](../I18N.md) dosyasına bakın.

AgentDeck, AGPL altında ve bundan ayrı olarak ticari bir lisansla sunulmaktadır. Bunun mümkün kalması için katkıda bulunanlardan, ilk pull request'lerinde bir katkıda bulunan lisans sözleşmesi (CLA) imzalamaları istenir.

## Lisans

AgentDeck, [GNU Affero General Public License v3.0](../../LICENSE) kapsamında özgür bir yazılımdır. Yazılımı kullanabilir, inceleyebilir, değiştirebilir ve paylaşabilirsiniz. Değiştirilmiş bir sürümü başkaları için ağ üzerinden çalıştırırsanız, o sürümün kaynak kodunu onlara aynı lisans altında sunmanız gerekir.

AGPL'nin yükümlülüklerini içermeyen ticari bir lisans için (örneğin AgentDeck'i kapalı kaynaklı bir ürünün içinde dağıtmak istiyorsanız) depo üzerinden iletişime geçin.

Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code ve Antigravity, ilgili sahiplerinin ürünleridir. AgentDeck'in bunlarla herhangi bir bağlantısı yoktur ve bunlar tarafından onaylanmamıştır; sizin kurduğunuz kopyaları çalıştırır.
