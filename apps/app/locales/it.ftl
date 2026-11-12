# Testi dell'interfaccia di StarBooru, italiano.

posts-count = { $count ->
    [one] { $count } post
   *[other] { $count } post
}

## Shared

retry = Riprova
loading = Caricamento
notfound-title = Qui non c'è niente
notfound-body = Non esiste una pagina { $path }.
notfound-back = Torna alla galleria
theme-system = Sistema
theme-light = Chiaro
theme-dark = Scuro

## Time

ago-now = adesso
ago-minutes =
    { $count ->
        [one] 1 minuto fa
       *[other] { $count } minuti fa
    }
ago-hours =
    { $count ->
        [one] 1 ora fa
       *[other] { $count } ore fa
    }
ago-yesterday = ieri
ago-days = { $count } giorni fa
date-short = { $day } { $month } { $year }
month-1 = gen
month-2 = feb
month-3 = mar
month-4 = apr
month-5 = mag
month-6 = giu
month-7 = lug
month-8 = ago
month-9 = set
month-10 = ott
month-11 = nov
month-12 = dic

## Navigation

nav-main = Principale
nav-gallery = Galleria
nav-explore = Esplora
nav-tags = Tag
nav-upload = Carica
nav-settings = Impostazioni
nav-settings-tab = Opzioni
search-placeholder = Cerca tag, ad es. cat rating:safe
demo-tag = Post con questo tag
demo-tag-all = Post con tutti i tag elencati
demo-tag-not = Il meno esclude un tag
demo-figure = Una persona li ha tutti
demo-rating = Solo questa classificazione
demo-rating-any = Una di queste classificazioni
demo-rating-not = Tutto tranne l'esplicito
demo-type = Solo video
demo-date = Di marzo 2024
demo-date-range = Dal 2023 alla fine del 2024
demo-date-age = Di meno di 7 giorni fa
demo-size = Larghi più di 1920 pixel
demo-ratio = Larghezza su altezza 16:9
demo-color = Ha questo colore
demo-text = Questa parola è scritta nell'immagine
demo-order-score = Prima il punteggio più alto
demo-order-random = Un ordine nuovo ogni volta
demo-combined = Tutto insieme
search-label = Cerca tag

## Sign-in

login-unreachable = Impossibile raggiungere il server: { $error }
login-wrong = Nome utente o password errati.
login-tagline = La tua raccolta privata di immagini
login-first-user = Questo server non ha ancora account. L'account che crei ora diventa l'amministratore.
login-server = Server
login-username = Nome utente
login-password = Password
login-wait = Attendi…
login-create-admin = Crea l'account amministratore
login-create = Crea account
login-sign-in = Accedi
login-have-account = Ho già un account
login-want-account = Crea un account
login-weak-password = Questa password è facile da indovinare. Prova con una più lunga, o con qualche parola che non c'entra con le altre.

## Shared

load-more = Carica altri
sure = Sicuro?
not-signed-in = Accesso non effettuato
no-matches = Nessun risultato
nothing-matches = Nessun risultato per “{ $query }”.

## Recycle bin

trash-title = Cestino
trash-purged =
    { $count ->
        [one] 1 post eliminato definitivamente.
       *[other] { $count } post eliminati definitivamente.
    }
trash-empty = Svuota il cestino
trash-empty-confirm = Eliminare tutto definitivamente?
trash-lead = I post eliminati restano qui finché non li ripristini o li elimini definitivamente. L'eliminazione definitiva rimuove anche il file dal server.
trash-none = Il cestino è vuoto
trash-restore = Ripristina
trash-restore-post = Ripristina il post { $id }
trash-purge = Elimina definitivamente
trash-purge-post = Elimina definitivamente il post { $id }

## Explore

explore-window = Tendenze di
explore-today = Oggi
explore-week = Settimana
explore-month = Mese
explore-year = Anno
explore-phrase-today = oggi
explore-phrase-week = questa settimana
explore-phrase-month = questo mese
explore-phrase-year = quest'anno
explore-phrase-days = negli ultimi { $count } giorni
explore-nothing = Ancora niente da esplorare
explore-nothing-body = Carica immagini o video e i migliori compariranno qui.
explore-everything = Esplora tutto
explore-trending = Di tendenza
explore-no-upvotes = Nessun voto positivo { $when }. Vota i post che ti piacciono e compariranno qui.
explore-popular-tags = Tag popolari
explore-tags-note = sui post aggiunti { $when }
explore-added = Post aggiunti { $when }
explore-on-this-day = Accadde oggi
explore-random = Scelti a caso
explore-shuffle = Mescola
explore-filter-placeholder = Restringi, ad es. -rating:e
explore-filter-label = Restringi con una ricerca

## Shared

show-more = Mostra altri
tags-count =
    { $count ->
        [one] 1 tag
       *[other] { $count } tag
    }
post-alt = Post { $id }
role-admin = amministratore
role-user = utente

## Settings

settings-appearance = Aspetto
settings-theme = Tema
settings-language = Lingua
settings-language-system = Sistema
settings-accent = Colore principale
settings-accent-custom = Colore principale personalizzato
accent-no-hue = I grigi non hanno una tonalità da cui ricavare i colori. Scegli un colore più deciso.
accent-violet = Viola
accent-pink = Rosa
accent-coral = Corallo
accent-amber = Ambra
accent-green = Verde
accent-teal = Verde acqua
accent-blue = Blu
accent-red = Rosso
settings-library = Raccolta
settings-trash-note = Ripristina o elimina definitivamente i post
settings-dup-note = Trova i post simili e tieni la copia migliore
settings-help-legal = Aiuto e note legali
settings-help-note = Sintassi delle ricerche, esempi e scorciatoie da tastiera
settings-privacy-note = Cosa viene salvato, dove e per quanto tempo
settings-terms-note = Le regole per usare questo server
settings-account = Account
settings-signed-in-as = Accesso come
settings-role = Ruolo
settings-server-version = Versione del server
settings-sign-out = Esci
about-title = Informazioni su StarBooru
about-made-by = Creato da
about-icons = Icone
about-lettering = Lettere del logo
about-version = Versione dell'app
about-credits = Tutti i crediti
about-credits-note = Tutti gli autori delle opere usate da StarBooru, con le licenze
password-current = Password attuale
password-new = Nuova password
password-too-short = Almeno { $count } caratteri.
password-repeat = Ripeti la nuova password
password-mismatch = Le password non coincidono.
password-changed = Password cambiata. Gli altri tuoi dispositivi sono stati disconnessi.
password-changing = Modifica in corso…
password-change = Cambia password
password-strength-weak = Facile da indovinare
password-strength-fair = Potrebbe essere più sicura
password-strength-strong = Password sicura
sessions-title = Dispositivi connessi
sessions-unknown = Client sconosciuto
sessions-this-device = Questo dispositivo
sessions-times = Attivo { $active } · accesso { $created }
sessions-revoke-others = Esci da tutti gli altri dispositivi
sessions-revoke = Disconnetti questo dispositivo

## Duplicates

dup-title = Trova duplicati
dup-matching = Somiglianza
dup-strict = Rigida
dup-normal = Normale
dup-loose = Ampia
dup-lead = Post che si somigliano, prima i più simili. Tieni la copia migliore e le altre finiscono nel cestino.
dup-unhashed =
    { $count ->
        [one] 1 post non può ancora essere confrontato: non ha una miniatura o la sua impronta è in calcolo.
       *[other] { $count } post non possono ancora essere confrontati: non hanno una miniatura o la loro impronta è in calcolo.
    }
dup-none = Nessun duplicato trovato
dup-none-try-loose = Prova la somiglianza Ampia per trovare le copie modificate.
dup-none-loose = Qui niente si somiglia.
dup-groups =
    { $count ->
        [one] 1 gruppo
       *[other] { $count } gruppi
    }
dup-some-of = { $shown } di { $count } post
dup-keep-which = Post da tenere
dup-merge = Copia i loro tag sul post tenuto
dup-dismiss = Non sono duplicati
dup-resolve =
    { $count ->
        [one] Tienine uno, cestina l'altro
       *[other] Tienine uno, cestina gli altri { $count }
    }
dup-unknown-size = Dimensioni sconosciute
dup-keep = Tieni
dup-identical = Identici
dup-nearly-identical = Quasi identici
dup-very-similar = Molto simili
dup-similar = Simili

## Ratings

rating-safe = sicuro
rating-sensitive = delicato
rating-questionable = discutibile
rating-explicit = esplicito

## Search cheat sheet

help-title = Guida alla ricerca
help-try = Prova una ricerca
help-try-label = Ricerca da spiegare
help-search = Cerca
help-search-for = Cerca { $term }
help-tags = Tag
help-tag = ha il tag
help-tag-all = ha tutti i tag elencati
help-tag-not = non ha il tag
help-tag-tree = ha footwear o un tag sotto di esso, come boots o sneakers
help-figure = una persona nell'immagine ha tutti questi tag; girl, boy o other dice chi. Un meno davanti trova i post dove nessuno li ha
help-tag-category = il tag some_name; quando tagghi, il prefisso lo rende un tag artista (anche character:, copyright: e meta:)
help-tag-spaces = due tag: gli spazi separano i termini, quindi scrivi blue_sky
help-tag-meta = post ad alta risoluzione senza audio. Il server aggiunge da solo tag meta come lowres, highres, absurdres, wide_image, tall_image, hd, full_hd, 4k, 8k, horizontal_image, vertical_image, square_image, animated, video, sound e long_video
help-filters = Filtri
help-rating = classificato sicuro, delicato (s), discutibile (q) o esplicito (e)
help-rating-any = una qualsiasi delle classificazioni elencate
help-rating-not = qualsiasi classificazione tranne esplicito
help-type = immagini statiche; anche type:animation e type:video
help-deleted = post nel cestino
help-date = la data nel file, o quella di caricamento se non ne ha; anche date:2024 e date:2024-03-15
help-date-range = dall’inizio del primo periodo alla fine dell’ultimo; date:2023.. e date:..2024 lasciano aperto un estremo
help-date-compare = dopo quel periodo; anche >=, < e <=
help-date-age = con più di 30 giorni; date:<7d ha meno di 7 giorni. Anche h, w, mo e y
help-size = più largo di 1920 pixel; anche height:, >=, <, <=, un valore esatto width:1920 e un intervallo width:1000..2000
help-mpixels = più di 8 megapixel; mpixels:8 da solo vuol dire circa 8
help-ratio = larghezza su altezza, come 16:9 o 1.78, con margine di 0,01; ratio:1 è quadrato e ratio:>1 è più largo che alto
help-size-not = qualsiasi dimensione tranne queste; un post di dimensione ignota resta
help-color = ha questo colore o, se nessun post ce l'ha, il più vicino salvato; anche orange, yellow, green, teal, blue, purple, pink, brown, black, gray e white
help-color-hex = qualsiasi colore, in esadecimale; anche #2060e0 e 26e
help-color-not = non ha questo colore
help-exact = di solito un tag sconosciuto viene corretto con il più vicino in uso; così i tag si cercano esattamente come scritti
help-text = Testo nelle immagini
help-text-word = il testo letto nell’immagine contiene la parola (maiuscole o minuscole)
help-text-phrase = contiene tutte queste parole; le virgolette le tengono insieme
help-text-not = il testo letto nell’immagine non contiene la parola
help-sorting = Ordinamento
help-order-new = prima i più recenti (predefinito); order:old per i più vecchi
help-order-score = prima il punteggio più alto
help-order-size = prima i file più grandi
help-order-pixels = prima la risoluzione più alta; anche order:mpixels
help-order-longside = prima il lato più lungo
help-order-tags = prima quelli con più tag
help-order-updated = prima quelli modificati più di recente
help-order-direction = aggiungi _asc o _desc per scegliere la direzione
help-order-random = un ordine casuale nuovo ogni volta
help-combined = Combinazioni
help-combined-cats = gatti adatti a tutti senza cani, prima i migliori
help-combined-videos = i video più grandi
help-tips = Buono a sapersi
help-tip-normalise = I tag sono in minuscolo, e spazi o trattini bassi ripetuti diventano un solo _.
help-tip-aliases = Un alias trova il suo tag, e i tag implicati vengono aggiunti quando tagghi un post.
help-tip-terms = Una ricerca contiene al massimo 40 termini.
help-tip-saved = Aggiungi una ricerca ai preferiti dalla galleria per salvarla.
help-shortcuts = Scorciatoie da tastiera
key-prev-next = post precedente o successivo
key-edit = modifica tag e classificazione
key-fullscreen = schermo intero
key-zoom = ingrandisci o riduci
key-fit = adatta allo schermo
key-escape = annulla lo zoom, esci dallo schermo intero, poi torna alla galleria
key-double-click = Doppio clic
key-double-click-action = ingrandisci un punto, di nuovo per adattare
key-shift-click = Maiusc + clic
key-shift-click-action = seleziona un intervallo in modalità selezione
explain-deleted = Nel cestino
explain-tagged = Con il tag { $tag }
explain-tagged-all = Con tutti i tag { $tags }
explain-not-tagged = Senza il tag { $tag }
explain-tagged-none = Senza nessuno dei tag { $tags }
explain-rated = Classificati { $ratings }
explain-not-rated = Non classificati { $ratings }
list-or = { $rest } o { $last }
explain-images = Solo immagini statiche
explain-animations = Solo animazioni
explain-videos = Solo video
explain-figure = Una persona ha tutti questi tag: { $tags }
explain-no-figure = Nessuno ha tutti questi tag: { $tags }
explain-figure-kind = Una persona ({ $kind }) ha tutti questi tag: { $tags }
explain-no-figure-kind = Nessuna persona ({ $kind }) ha tutti questi tag: { $tags }
explain-figure-only = Una persona segnata come { $kind }
explain-no-figure-only = Nessuna persona segnata come { $kind }
explain-text = Il testo nell’immagine contiene “{ $text }”
explain-not-text = Il testo nell’immagine non contiene “{ $text }”
explain-date = Data { $value }
explain-not-date = Data diversa da { $value }
explain-size = { $field }: { $value }
explain-not-size = { $field }: non { $value }
explain-color = Ha il colore { $value }, o il più vicino salvato
explain-not-color = Non ha il colore { $value }
explain-exact = I tag si cercano esattamente come scritti
explain-size-width = Larghezza
explain-size-height = Altezza
explain-size-mpixels = Megapixel
explain-size-ratio = Proporzioni
explain-every = Tutti i post
explain-order-new = Prima i più recenti
explain-order-old = Prima i più vecchi
explain-order-random = In ordine casuale
explain-order-score-desc = Per punteggio, prima il più alto
explain-order-score-asc = Per punteggio, prima il più basso
explain-order-size-desc = Per dimensione del file, prima la più grande
explain-order-size-asc = Per dimensione del file, prima la più piccola
explain-order-pixels-desc = Per risoluzione, prima la più alta
explain-order-pixels-asc = Per risoluzione, prima la più bassa
explain-order-longside-desc = Per lato più lungo, prima il più lungo
explain-order-longside-asc = Per lato più lungo, prima il più corto
explain-order-tags-desc = Per numero di tag, prima i più numerosi
explain-order-tags-asc = Per numero di tag, prima i meno numerosi
explain-order-updated-desc = Per ultima modifica, prima la più recente
explain-order-updated-asc = Per ultima modifica, prima la meno recente

## Privacy Policy and Terms of Service

privacy-1 = StarBooru è un software che gira su un server scelto da te o da qualcuno di cui ti fidi. Niente di ciò che salvi viene inviato al progetto StarBooru o a chiunque altro.
privacy-2 = Chi è responsabile
privacy-3 = La persona o l'organizzazione che gestisce questo server (il gestore) decide chi può registrarsi ed è responsabile dei dati che contiene. Le domande sui tuoi dati vanno rivolte a lei.
privacy-4 = Cosa salva il server
privacy-5 = Il tuo account: nome utente, ruolo e un hash Argon2id con sale della tua password. La password in sé non viene mai salvata. Se accedi tramite il provider di identità del gestore, il server conserva l'identificativo che quel provider ti assegna e il nome utente o l'email che riporta, per riconoscerti la volta successiva.
privacy-6 = Le tue sessioni: un hash di ogni token di accesso, il tipo di dispositivo da cui proviene (ad esempio “StarBooru app on Linux” o “Firefox on Android”), quando è stata creata e usata l'ultima volta, e quando scade. Le chiavi API che crei sono conservate allo stesso modo: un hash della chiave, il suo nome e quando è stata usata l'ultima volta.
privacy-7 = Ciò che aggiungi: i file caricati con dimensione, risoluzione, checksum e un'impronta visiva per la ricerca dei duplicati, oltre a tag, classificazioni, fonti, voti e ricerche salvate. Per ogni link di condivisione che crei: i post che contiene, il titolo, la scadenza, e quante volte e quando è stato aperto l'ultima volta (non da chi).
privacy-8 = Per rallentare i tentativi di indovinare le password, il server conta gli accessi falliti per account e per indirizzo di rete, e le registrazioni per indirizzo. Questi conteggi restano in memoria al massimo un'ora e non vengono mai scritti su disco.
privacy-9 = Cosa resta sul tuo dispositivo
privacy-10 = L'app ricorda l'indirizzo del server, il tuo token di accesso e le tue preferenze, come il tema. Uscendo il token viene rimosso.
privacy-11 = Cosa StarBooru non fa
privacy-12 = Niente statistiche d'uso, pubblicità o tracciamento, e nessun servizio di terze parti oltre a un provider di identità che il gestore sceglie di configurare.
privacy-13 = Il server contatta altri siti solo quando importi da un URL o da un altro booru, e il provider di identità del gestore quando accedi tramite esso. Le importazioni usano un user agent generico e nessuno dei tuoi dati. Se un amministratore configura un fornitore di IA, le miniature dei post gli vengono inviate per suggerire i tag, il testo letto in un’immagine quando lo traduci, e l’immagine stessa quando se ne traducono le vignette di fumetti o manga; un fornitore sulla rete dell’operatore le tiene lì. Mentre si importa un’intera raccolta, la chiave che hai dato per quel sito resta sul server, e in ogni backup fatto nel frattempo; viene cancellata quando l’importazione finisce.
privacy-14 = Il contenuto dei file, le password e i token non vengono mai scritti nei log del server.
privacy-15 = Chi può vedere i tuoi post
privacy-16 = Chiunque abbia un account su questo server può vedere i post che contiene. I link ai file forniti all'app sono firmati e scadono, quindi un link copiato smette di funzionare entro 12 ore. Se crei un link di condivisione, chiunque lo abbia può vedere i post che contiene, senza i loro tag, finché non scade o non lo rimuovi.
privacy-17 = Conservazione ed eliminazione
privacy-18 = I post eliminati finiscono nel cestino. Vengono cancellati quando un amministratore lo svuota, o automaticamente dopo il periodo stabilito dal gestore.
privacy-19 = Le sessioni terminano quando esci, quando scadono o quando disconnetti gli altri dispositivi dalle Impostazioni. Cambiare la password disconnette tutti gli altri dispositivi.
privacy-20 = Gli amministratori di questo server vedono di ogni account il nome utente, il ruolo, quando si è iscritto ed è stato attivo l'ultima volta, e quanto ha caricato. Possono impostare un limite di spazio, reimpostare una password dimenticata, disattivare un account o eliminarlo. Per eliminare il tuo account, chiedi a uno di loro.
privacy-21 = Il gestore può conservare dei backup, che seguono i suoi tempi.
terms-1 = Questi termini regolano l'uso di questo server StarBooru. Il gestore che lo amministra può aggiungerne altri.
terms-2 = Il tuo account
terms-3 = Non condividere la tua password e disconnetti i dispositivi che non usi più (Impostazioni › Dispositivi connessi).
terms-4 = Il primo account di un server ne è l'amministratore. Gli amministratori possono gestire tag, alias e implicazioni, svuotare il cestino, cambiare le impostazioni del server e gestire gli account: ruoli, limiti di spazio, reimpostazione delle password, disattivazione ed eliminazione.
terms-5 = I tuoi contenuti
terms-6 = Carica solo ciò che hai il diritto di conservare e condividere con le altre persone di questo server, e ciò che è legale dove si trova il server.
terms-7 = Mantieni ogni diritto su ciò che carichi. Consenti al server di conservarlo, crearne miniature e anteprime, e mostrarlo agli account di questo server.
terms-8 = Gli originali sono conservati byte per byte. Classifica i post con onestà, così i filtri per classificazione funzionano per tutti.
terms-9 = Uso corretto
terms-10 = Non tentare di accedere ad account o dati altrui, né di aggirare i limiti del server.
terms-11 = Non sovraccaricare il server con richieste automatiche.
terms-12 = Nessuna garanzia
terms-13 = StarBooru è fornito così com'è, senza garanzie di alcun tipo. Tieni una tua copia di tutto ciò che non puoi permetterti di perdere. Il gestore può sospendere gli account che violano questi termini e può modificarli; la versione in vigore è sempre su questa pagina.

## Legal links

privacy-title = Informativa sulla privacy
terms-title = Termini di servizio
legal-nav = Note legali
legal-back = Torna a StarBooru
legal-privacy-short = Privacy
legal-terms-short = Termini

## Upload

upload-skipped =
    { $count ->
        [one] Saltato 1 file che non è un'immagine o un video.
       *[other] Saltati { $count } file che non sono immagini o video.
    }
upload-urls-only = Inserisci indirizzi che iniziano con http:// o https://.
upload-drop = Trascina qui immagini o video
upload-drop-now = Rilascia per caricare
upload-or-choose = oppure fai clic per scegliere i file
upload-folder = Scegli una cartella
upload-folder-tags = Tagga i file con il nome della loro cartella
upload-urls-placeholder = Incolla indirizzi di immagini o video
upload-urls-label = Indirizzi di immagini o video
add = Aggiungi
upload-shared-tags = Tag per tutti i file
rating = Classificazione
upload-clear = Svuota l'elenco
upload-start =
    { $count ->
        [0] Carica
        [one] Carica 1 file
       *[other] Carica { $count } file
    }
upload-from-web = Dal web
upload-queued = In coda
upload-uploading = In caricamento
post-number = Post #{ $id }
upload-failed = Non riuscito
upload-file-tags-placeholder = Tag solo per questo file
upload-file-tags-label = Tag per { $name }
upload-unreadable = impossibile leggere il file: { $error }
upload-ready = Il prossimo
upload-remove = Rimuovi { $name }
upload-device-hint = I caricamenti continuano mentre usi le altre pagine, ma si fermano se chiudi StarBooru.
upload-jobs-title = Importazioni sul server
upload-jobs-hint = Li scarica il tuo server, quindi finiscono anche se chiudi StarBooru.
upload-done = Importato
upload-job-running = In importazione
upload-job-cancel = Annulla
upload-jobs-clear = Togli i completati
activity-signed-out = Non hai effettuato l'accesso.
activity-pending =
    { $count ->
        [one] 1 caricamento o importazione in corso
       *[other] { $count } caricamenti e importazioni in corso
    }
activity-uploaded =
    { $count ->
        [one] 1 file caricato
       *[other] { $count } file caricati
    }
activity-imported =
    { $count ->
        [one] 1 post importato
       *[other] { $count } post importati
    }
activity-known =
    { $count ->
        [one] 1 già presente
       *[other] { $count } già presenti
    }
activity-failed =
    { $count ->
        [one] 1 non riuscito
       *[other] { $count } non riusciti
    }

## Tags

category-general = Generale
category-artist = Artista
category-character = Personaggio
category-copyright = Copyright
category-meta = Meta
section-people = Persone
section-hair = Capelli
section-eyes = Occhi
section-skin = Pelle
section-body = Corpo
section-expression = Espressione
section-pose = Posa e inquadratura
section-clothing = Abbigliamento
section-setting = Ambientazione
section-style = Stile
section-other = Altro
people-title = Persone
people-none = Nessuno è ancora segnato. Aggiungendo le persone nell'immagine tagghi vestiti e aspetto di ognuna, e 1girl, 2boys e solo restano giusti.
people-hint = I loro tag vanno anche sul post. I conteggi come 1girl e solo seguono le persone. Separa i tag con spazi.
people-add = Aggiungi una persona
people-edit = Modifica
people-remove = Togli questa persona
people-kind = Chi
people-girl = Ragazza
people-boy = Ragazzo
people-other = Altro
people-numbered = { $kind } { $n }
people-character = Personaggio
people-cosplay = In cosplay da
people-as = come
people-clothing = Vestiti
people-looks = Aspetto
slot-head = Testa
slot-body = Busto
slot-bottom = Sotto e gambe
slot-shoes = Scarpe
slot-wear = Altri vestiti
slot-shape = Corporatura
slot-eyes = Occhi
slot-skin = Pelle
slot-hair = Capelli
slot-pose = Posa
slot-other = Altro
tags-tree = Posizione nell'albero
tags-tree-hint = Cercare un tag trova anche i tag sotto di esso, e la cima dell'albero è la sezione in cui vengono raggruppati i tag di un post.
tags-tree-top = In cima: niente sopra.
tags-parent = Tag genitore
tags-move = Sposta sotto
tags-move-top = Porta in cima
tags-children = Sotto di esso
tags-children-none = Ancora niente sotto.
tags-sort = Ordina i tag
tags-most-used = Più usati
tags-a-z = A–Z
tags-filter-label = Filtra i tag
tags-filter-placeholder = Filtra per nome o alias
tags-none-yet = Ancora nessun tag
tags-none-match = Nessun tag corrisponde
tags-appear = I tag compaiono qui quando i post li usano.
tags-missing = Nessun tag chiamato “{ $name }”
tags-all = Tutti i tag
tags-browse =
    { $count ->
        [one] Sfoglia 1 post
       *[other] Sfoglia { $count } post
    }
tags-is-alias = “{ $name }” è un alias di questo tag.
tags-category = Categoria
tags-aliases = Alias
tags-aliases-hint = Altri nomi per questo tag. Taggare o cercare con un alias usa questo tag. Aggiungere un alias che è già un tag lo unisce a questo.
tags-aliases-none = Nessun alias.
tags-implies = Implica
tags-implies-hint = I post con questo tag ricevono anche questi tag, compresi quelli taggati in precedenza.
tags-implies-none = Non implica altri tag.
tags-implied-by = Implicato da
tags-implied-by-hint = Tag che aggiungono questo.
tags-implied-by-none = Nessun tag implica questo.
tags-new-alias = Nuovo alias
tags-implied-tag = Tag implicato
tags-remove = Rimuovi
tags-remove-named = Rimuovi { $name }
tags-admins-only = Solo gli amministratori possono modificarlo.

## Post

post-in-trash = Questo post è nel cestino.
post-actions = Azioni sul post
post-back = Indietro
post-upvote = Voto positivo
post-downvote = Voto negativo
post-download = Scarica
post-edit = Modifica
post-edit-tags = Modifica i tag
post-delete = Elimina
post-delete-title = Sposta nel cestino
post-tags = Tag
post-tags-hint = Metti artist:, character:, copyright: o meta: davanti a un tag per sceglierne la categoria.
post-did-you-mean = Intendevi?
post-replace-tag = Sostituisci { $from } con { $to }
cancel = Annulla
save = Salva
post-no-tags = Ancora nessun tag.
post-details = Dettagli
post-related = Simili
post-size = Dimensione
post-dimensions = Risoluzione
post-length = Durata
post-type = Tipo
post-added = Aggiunto
post-file-date = Data del file
post-colors = Colori
post-color-search = Cerca { $color }
post-source = Fonte
post-prev-title = Precedente (←)
post-prev = Post precedente
post-next-title = Successivo (→)
post-next = Post successivo
zoom-out-title = Riduci (−)
zoom-out = Riduci
zoom-fit = Adatta
zoom-fit-title = Adatta (0)
zoom-reset = Ripristina lo zoom
zoom-in-title = Ingrandisci (+)
zoom-in = Ingrandisci
fullscreen-exit-title = Esci da schermo intero (F)
fullscreen-exit = Esci da schermo intero
fullscreen-title = Schermo intero (F)
fullscreen = Schermo intero

## Gallery

gallery-tag-info = Informazioni su questo tag
view-title = Disposizione della galleria
view-shape = Forma delle miniature
view-square = Quadrate
view-landscape = Orizzontali
view-portrait = Verticali
view-staggered = Sfalsate
view-staggered-by = Forme sfalsate
view-everything = Tutte
view-columns = { $count ->
    [one] 1 colonna
   *[other] { $count } colonne
}
view-per-row = Circa { $count } per riga
view-columns-auto = Automatico
gallery-stop-selecting = Termina la selezione
gallery-select = Seleziona i post
gallery-syntax = Sintassi di ricerca
search-corrected = Ortografia corretta: { $tags }
search-exact = Cerca esattamente come scritto
gallery-empty = Ancora nessun post
gallery-empty-body = Carica immagini o video per iniziare la tua raccolta.
gallery-try-fewer = Nessun risultato per questa ricerca. Prova con meno tag.
bulk-label = Modifica i post selezionati
bulk-selected =
    { $count ->
        [one] 1 selezionato
       *[other] { $count } selezionati
    }
bulk-select-none = Deseleziona tutto
bulk-select-all = Seleziona tutto
bulk-tags-placeholder = Tag: cat -dog
bulk-tags-title = Tag da aggiungere; metti - davanti a un tag per rimuoverlo
bulk-tags-label = Tag da aggiungere, o da rimuovere con un meno davanti
bulk-keep-rating = Mantieni la classificazione
bulk-apply = Applica
bulk-delete-confirm-label =
    { $count ->
        [one] Eliminare 1 post?
       *[other] Eliminare { $count } post?
    }
bulk-delete-confirm = Eliminare { $count }?
bulk-updated =
    { $count ->
        [one] Aggiornato 1 post
       *[other] Aggiornati { $count } post
    }
bulk-moved =
    { $count ->
        [one] Spostato 1 post nel cestino
       *[other] Spostati { $count } post nel cestino
    }
bulk-skipped =
    { $count ->
        [one] saltato 1 post
       *[other] saltati { $count } post
    }
bulk-skipped-undeletable =
    { $count ->
        [one] saltato 1 post che non puoi eliminare
       *[other] saltati { $count } post che non puoi eliminare
    }
bulk-done = { $done }.
bulk-done-skipped = { $done }; { $skipped }.
search-forget = Dimentica questa ricerca
search-save = Salva questa ricerca
saved-searches = Ricerche salvate
sort = Ordina
sort-by = Ordina per
color-search = Cerca per colore
color-wheel = Scegli un colore dalla ruota
color-hex = Colore esadecimale
color-hex-apply = Usa questo colore
color-hex-invalid = Usa 3 o 6 cifre esadecimali, ad es. #2060e0
color-clear = Qualsiasi colore
color-red = Rosso
color-orange = Arancione
color-yellow = Giallo
color-green = Verde
color-teal = Verde acqua
color-blue = Blu
color-purple = Viola
color-pink = Rosa
color-brown = Marrone
color-black = Nero
color-gray = Grigio
color-white = Bianco
sort-new = Più recenti
sort-old = Meno recenti
sort-score = Punteggio più alto
sort-updated = Modificati di recente
sort-size = File più grandi
sort-size-asc = File più piccoli
sort-pixels = Risoluzione più alta
sort-longside = Lato più lungo
sort-tags = Più tag
sort-tags-asc = Meno tag
sort-random = Casuale

## API keys

keys-title = Chiavi API
keys-lead = Permetti a script e altri strumenti di usare StarBooru a tuo nome. Una chiave non può cambiare la password, gli accessi o le chiavi.
keys-copy-now = Copia ora la chiave “{ $name }”. Non verrà più mostrata.
keys-token = Nuova chiave API
keys-copy = Copia
keys-copied = Copiata
keys-none = Ancora nessuna chiave API.
keys-read-only = Sola lettura
keys-full = Accesso completo
keys-expired = Scaduta
keys-used = Usata { $when }
keys-never-used = Mai usata
keys-no-expiry = Non scade
keys-expired-on = Scaduta il { $date }
keys-expires-on = Scade il { $date }
keys-name = Nome
keys-name-placeholder = Chi la userà, ad es. script di backup
keys-access = Accesso
keys-lifetime = Scadenza
keys-never-expires = Mai
keys-days =
    { $count ->
        [one] 1 giorno
       *[other] { $count } giorni
    }
keys-create = Crea chiave
keys-delete = Rimuovi la chiave
keys-delete-named = Rimuovi la chiave { $name }

## Administration

admin-title = Amministrazione
admin-only = Solo gli amministratori possono aprire questa pagina.
admin-tab-overview = Panoramica
admin-tab-accounts = Account
admin-tab-server = Server
admin-on = Attivo
admin-off = Disattivato
admin-posts = Post
admin-images = Immagini
admin-animations = Animazioni
admin-videos = Video
admin-bin = Nel cestino
admin-tags = Tag
admin-accounts = Account
admin-media = Media su disco
admin-database = Dimensione del database
admin-uploads-title = Caricamenti, ultimi 30 giorni
admin-uploads-total =
    { $count ->
        [one] 1 caricamento negli ultimi 30 giorni
       *[other] { $count } caricamenti negli ultimi 30 giorni
    }
admin-server-title = Server
admin-running = Avviato
admin-upload-limit = Caricamento massimo
admin-bin-size = Dimensione del cestino
admin-disabled-accounts = Account disattivati
admin-video-thumbs = Miniature video (ffmpeg)
admin-url-import = Importazione da URL
admin-accounts-lead = Tutti gli account di questo server, cosa hanno caricato e quando sono stati attivi l'ultima volta.
admin-disabled = Disattivato
admin-you = Tu
admin-active = attivo { $when }
admin-never-active = mai attivo
admin-joined = iscritto il { $date }
admin-manage = Gestisci
admin-manage-named = Gestisci { $name }
admin-role-saved = Ruolo cambiato.
admin-limit = Limite di spazio (GB)
admin-limit-none = Nessun limite
admin-limit-hint = I caricamenti che lo supererebbero vengono rifiutati. Lascia vuoto per nessun limite.
admin-limit-save = Imposta
admin-limit-saved = Limite di spazio salvato.
admin-limit-invalid = Scrivi un numero di GB o lascia vuoto.
admin-password-new = Nuova password
admin-password-hint = Per chi ha dimenticato la propria. Lo disconnette ovunque; le sue chiavi API continuano a funzionare.
admin-password-reset = Reimposta
admin-password-reset-done = Password reimpostata. Comunica quella nuova.
admin-disable = Disattiva account
admin-enable = Riattiva account
admin-disabled-done = Account disattivato e disconnesso.
admin-enabled-done = Account riattivato.
admin-delete = Elimina account
admin-delete-confirm = Eliminare definitivamente?
admin-delete-note = L'eliminazione rimuove l'account, i suoi voti, le ricerche salvate e le chiavi. I suoi post restano.
admin-server-lead = Si applicano subito e hanno la precedenza sulle variabili d'ambiente del server.
admin-name = Nome del server
admin-name-hint = Mostrato nella pagina di accesso.
admin-registration = Chiunque può creare un account
admin-registration-hint = Disattivato: possono accedere solo gli account esistenti.
admin-trash-days = Svuota il cestino dopo (giorni)
admin-trash-days-hint = I post nel cestino più vecchi di così vengono eliminati definitivamente, ogni ora. 0 li tiene finché non lo svuoti a mano.
admin-auto-tags = Aggiungi ai nuovi post i tag di ciò che mostra il file
admin-auto-tags-hint = Tag meta come highres, lowres, wide_image, video, sound e long_video. Ogni post viene taggato una volta sola, quindi un tag che togli resta tolto. Attivandolo vengono taggati anche i post più vecchi al prossimo avvio del server.
admin-save = Salva
admin-saved = Salvato
admin-ai-title = IA
admin-ai-unavailable = L'IA locale non può avviarsi: { $reason }
admin-ai-hw-threads = { $count } thread
admin-ai-hw-memory = { $total } di RAM
admin-ai-tagging = Suggerimento tag
admin-ai-translation = Traduzione
admin-ai-comics = Traduzione fumetti
admin-ai-mode-off = Spenta
admin-ai-mode-local = Locale
admin-ai-mode-api = API
admin-ai-speed = Velocità
admin-ai-preset-fast = Veloce
admin-ai-preset-balanced = Bilanciata
admin-ai-preset-accurate = Accurata
admin-ai-presets-off = Non disponibile su questa macchina: { $presets }
admin-ai-state-ready = Pronta
admin-ai-state-preparing = Preparazione…
admin-ai-state-downloading = Download { $percent }%
admin-ai-state-failed = Non riuscita
admin-ai-retry = Riprova
admin-ai-models = Modelli
admin-ai-no-memory = Memoria insufficiente
admin-ai-gpu = GPU
admin-ai-high-accuracy = Alta precisione
admin-ai-in-use = In uso
admin-ai-use = Usa
admin-ai-install = Installa
admin-ai-uninstall = Disinstalla
admin-ai-url = Indirizzo
admin-ai-model = Modello
admin-ai-key = Chiave API
admin-ai-key-set = Salvata
admin-ai-key-none = Nessuna
admin-ai-key-clear = Rimuovi chiave
admin-ai-test = Prova
admin-ai-testing = Prova in corso…
admin-ai-test-ok = Risposta in { $time }
admin-ai-autotag = Tagga in automatico i nuovi caricamenti
admin-ai-auto-translate = Traduci i nuovi fumetti in
admin-ai-auto-translate-hint = Codice della lingua, come it. Vuoto: no.
ai-suggest = Suggerisci tag
ai-suggesting = Chiedo all'IA…
ai-add-all = Aggiungi tutti
ai-new-tag = nuovo
ai-no-suggestions = Nessun suggerimento per questo post.
ai-suggest-people = Suggerisci persone
ai-no-people = L'AI non ha trovato persone.
ai-rating = Classificazione suggerita: { $rating }
settings-admin-note = Statistiche, account e impostazioni del server
settings-storage = Caricamenti
usage-of = { $used } su { $quota }

## Backups

admin-tab-backups = Backup
backups-title = Backup del database
backups-lead = Un backup contiene il database: post, tag, account e impostazioni. I file multimediali restano nella cartella dei dati, quindi copia anche quella.
backups-unavailable = Questo server non può fare backup: manca il client PostgreSQL (pg_dump). Installalo, oppure indica il percorso con STARBOORU_PG_DUMP.
backups-now = Fai un backup ora
backups-running = Backup in corso…
backups-none = Ancora nessun backup.
backups-automatic = Automatico
backups-download = Scarica
backups-delete = Elimina backup
backups-delete-named = Elimina il backup { $name }
backups-schedule-title = Backup automatici
backups-every = Ogni (giorni)
backups-every-hint = 0 disattiva i backup automatici.
backups-keep = Da tenere
backups-keep-hint = I backup automatici più vecchi vengono rimossi. Quelli fatti a mano restano finché non li elimini.
backups-restore-title = Ripristino
backups-restore = Ferma StarBooru, poi carica il backup nel suo database con pg_restore:
backups-restore-note = Poi riavvia StarBooru. La guida al deployment ha tutti i passaggi.

## Provider sign-in

login-or = oppure
login-with = Accedi con { $provider }
oidc-waiting = Completa l'accesso nel browser, poi torna qui.
oidc-finishing = Accesso in corso…
oidc-denied = L'accesso è stato annullato.
oidc-failed = L'accesso non è riuscito. Riprova o chiedi all'amministratore.
oidc-expired = Questo accesso è scaduto. Ricomincia.
oidc-no-browser = Impossibile aprire un browser: { $error }
oidc-close-tab = Fatto. Puoi chiudere questa scheda e tornare a StarBooru.
oidc-back-settings = Torna alle Impostazioni
oidc-back-sign-in = Torna all'accesso
signin-title = Modi di accesso
signin-password-set = Impostata
signin-password-none = Non impostata
signin-provider = Accesso unico
signin-times = Collegato { $linked } · ultimo uso { $used }
signin-linked = Collegato { $linked }
signin-last = È il tuo unico modo di accedere. Imposta una password prima di scollegarlo.
signin-link = Collega un account { $provider }
signin-unlink = Scollega
signin-unlink-named = Scollega { $name }
password-none = Questo account non ha ancora una password. Impostane una per accedere anche senza il provider.
password-set = Imposta password

## Share links

share-button = Condividi
share-title =
    { $count ->
        [one] Condividi questo post
       *[other] Condividi { $count } post
    }
share-lead = Chi ha il link può vedere questi post senza un account. Tag, voti e i tuoi dati restano nascosti.
share-name = Titolo (facoltativo)
share-name-placeholder = es. Foto delle vacanze
share-lifetime = Il link funziona per
share-until-removed = Finché non lo rimuovi
share-create = Crea link
share-ready = Link pronto. Chiunque lo abbia può aprirlo.
share-link = Link di condivisione
share-copy = Copia link
share-done = Fatto
share-close = Chiudi
share-posts =
    { $count ->
        [one] 1 post
       *[other] { $count } post
    }
shares-title = Link condivisi
shares-lead = I link creati con Condividi. Rimuoverne uno lo disattiva; un file già aperto da qualcuno può continuare a caricarsi per qualche ora.
shares-none = Ancora nessun link. Usa Condividi su un post, o seleziona dei post nella galleria.
shares-untitled = Link senza titolo
shares-made = Creato { $when }
shares-views =
    { $count ->
        [0] Mai aperto
        [one] Aperto una volta
       *[other] Aperto { $count } volte
    }
shares-delete = Rimuovi link
shares-delete-named = Rimuovi il link { $name }
shared-missing-title = Questo link non funziona
shared-missing-body = Potrebbe essere scaduto, o chi l'ha condiviso l'ha rimosso.
shared-from = Condiviso da { $server }
shared-until = disponibile fino al { $date }
shared-empty = Qui non c'è più niente.
shared-open = Elemento { $n }
shared-previous = Precedente
shared-next = Successivo

## Media editor

editor-open = Modifica immagine o video
editor-image-short = Modifica immagine
editor-video-short = Taglia
editor-title-image = Modifica immagine
editor-title-video = Taglia video
editor-tools = Strumenti
editor-turn-left = Ruota a sinistra
editor-turn-right = Ruota a destra
editor-flip-h = Rifletti da sinistra a destra
editor-flip-v = Rifletti dall'alto in basso
editor-crop = Ritaglia
editor-trim = Taglia
editor-history = Cronologia
editor-undo = Annulla
editor-redo = Ripeti
editor-undo-keys = Annulla (Ctrl+Z)
editor-redo-keys = Ripeti (Ctrl+Maiusc+Z)
editor-save-keys = Salva come nuovo post (Ctrl+S)
editor-ratio = Proporzioni
editor-ratio-free = Libero
editor-ratio-original = Originale
editor-ratio-custom = Personale
editor-ratio-swap = Verticale o orizzontale
editor-ratio-width = Larghezza
editor-ratio-height = Altezza
editor-straighten = Raddrizza
editor-straighten-reset = Torna a 0°
editor-turn-flip = Ruota e rifletti
editor-keep-original = Tieni l'originale
editor-keep-original-hint = La modifica si salva come nuovo post accanto all'originale
editor-saved = Salvato come nuovo post
editor-saved-binned = Originale spostato nel cestino
editor-undo-save = Annulla
editor-reset = Ricomincia
editor-start = Inizio (secondi)
editor-end = Fine (secondi)
editor-now = Usa il punto attuale
editor-now-title = Prendi il punto in cui si trova il video
editor-keeps = Tiene { $length } di { $total }.
editor-keyframe-hint = Dove il video lo permette la qualità originale resta intatta; altrimenti la clip viene ricodificata, e può volerci un po'.
editor-bad-time = Inserisci un inizio prima della fine, entro la durata del video.
editor-too-short = La clip deve durare almeno un decimo di secondo.
editor-no-change = Fai prima una modifica.
editor-save = Salva
editor-save-copy = Salva copia
editor-save-options = Cosa succede all'originale
editor-save-replace = Sostituisci l'originale
editor-save-replace-hint = L'originale va nel cestino

## Smaller copies

post-sample-note = Stai vedendo una copia più leggera. Ingrandendo si carica l’originale.
post-show-original = Mostra originale

## Testo nelle immagini (OCR)

post-ocr-title = Testo nell’immagine
post-ocr-copy = Copia
post-ocr-copied = Copiato
post-ocr-more = Mostra tutto
post-ocr-less = Mostra meno
post-ocr-translate = Traduci
post-ocr-translating = Traduzione…
post-ocr-original = Mostra originale
post-ocr-machine = Tradotto dall’IA; può contenere errori.

## Testo tradotto sull’immagine (fumetti e manga)

post-panels-make = Traduci l’immagine
post-panels-make-title = Traduce il testo dell’immagine e lo mostra sopra la figura. L’immagine viene inviata all’IA di traduzione del server.
post-panels-making = Traduzione dell’immagine…
post-panels-show = Mostra la traduzione
post-panels-hide = Nascondi la traduzione
post-panels-none = Nessun testo da tradurre.
settings-originals = Apri sempre gli originali nel visualizzatore
settings-originals-hint = Altrimenti le immagini grandi si aprono come copia più leggera, più veloce e con meno dati. L’originale si apre comunque quando ingrandisci, o quando lo schermo ha più pixel della copia.
settings-flush = Togli gli spazi e gli angoli arrotondati tra le immagini
settings-flush-hint = Le miniature stanno una accanto all’altra, con gli angoli squadrati.

## Media cache (desktop and Android)

cache-title = Spazio su questo dispositivo
cache-hint = Le miniature e le immagini che apri restano su questo dispositivo, così la volta dopo si vedono subito.
cache-used = Immagini salvate: { $size }
cache-limit = Tieni fino a
cache-off = Niente (disattivato)
cache-clear = Svuota

## Downloads (desktop)

notice-close = Chiudi
download-saved = Salvato in { $path }
download-failed = Download non riuscito: { $reason }
download-no-folder = nessuna cartella Download trovata

## Hand (tablet rail, viewer panel and tools)

settings-hand = Mano
settings-hand-hint = Mette la navigazione laterale su tablet e portatili, e il pannello e gli strumenti di zoom del visualizzatore, dal lato della mano che usi.
hand-either = Entrambe
hand-left = Sinistra
hand-right = Destra

## Battery saver

settings-saver = Risparmio batteria
settings-saver-hint = Quando è attivo, i video non partono da soli, il post successivo non viene caricato in anticipo e le animazioni sono ridotte. Automatico lo attiva quando la batteria è al 20% o meno e non è in carica, o quando il dispositivo risparmia dati.
settings-saver-now = Attivo ora: la batteria è scarica o il dispositivo sta risparmiando dati.
saver-auto = Automatico
saver-always = Sempre
saver-never = Mai

## Wallpaper colour

accent-system = Sfondo
accent-system-hint = Segui i colori che Android prende dallo sfondo

## Other boorus
boorus-open = Altri booru
boorus-title = Altri booru
boorus-hint = Cerca nei siti Danbooru, Gelbooru, Moebooru, e621 e Blombooru, scegli i post e importali con tag, classificazione e fonte.
boorus-site = Sito
boorus-custom = Un altro sito…
boorus-address = Indirizzo
boorus-api = Tipo di sito
boorus-account = Account (facoltativo)
boorus-account-hint = Alcuni siti rispondono, o permettono ricerche più lunghe, solo con il tuo account. Resta su questo dispositivo e viene inviato solo a quel sito, tramite il tuo server.
boorus-login = Nome utente
boorus-user-id = ID utente
boorus-key = Chiave API
boorus-password-hash = Hash della password
boorus-search = Cerca
boorus-search-label = Cerca su quel sito
boorus-search-placeholder = Tag, nella sintassi di quel sito
boorus-nothing = Nessun risultato su quel sito.
boorus-more = Carica altri
boorus-post = Post { $id }
boorus-known = Già presente
boorus-extra-tags = Tag in più per questi post
boorus-import-label = Importa i post selezionati
boorus-import = Importa { $count }
boorus-some-failed = Alcuni post non sono stati importati: { $error }
boorus-queued = Li sta importando il tuo server. Puoi lasciare questa pagina: finiranno comunque.

library-title = Importa da un altro booru
library-lead = Sposta qui un’intera raccolta da szurubooru, Shimmie2, Danbooru, Stash, Blombooru, Monbooru o LANBooru: ogni post con tag, classificazione, fonte, data e i tuoi “mi piace”, più alias, implicazioni, alberi di tag, e pool o album come tag. Il lavoro lo fa il tuo server, quindi puoi chiudere l’app. Importando di nuovo si aggiunge ciò che è nuovo, e un post che hai già riceve solo i tag. Alias e implicazioni arrivano solo se importa un amministratore. La tua chiave resta sul server mentre l’importazione è in corso e viene cancellata quando finisce.
library-left-out = Restano fuori: commenti, altri utenti e i loro account, note sulle immagini, titoli e descrizioni, punteggi del sito e l’ordine interno dei pool. StarBooru non ha un posto per questi dati.
library-source = Importa da
library-hint-szurubooru = Accedi con il tuo nome utente e un token dalla scheda Login tokens del tuo account. Senza, ottieni ciò che vedono gli ospiti.
library-hint-shimmie2 = Serve Ouroboros API attivata nell’Extension Manager di Shimmie, e una chiave API se gli ospiti non vedono tutto. I suoi alias e le regole di auto-tag arrivano se sono pubblici, tranne quelli di più parole. I pool non si possono leggere: Shimmie non ha un’API per questi.
library-hint-danbooru = Il tuo nome utente e la chiave API dal profilo. Senza, ottieni ciò che vedono gli ospiti e i tuoi preferiti restano fuori. Sui siti grandi sono milioni di post, quindi usalo sulla tua istanza.
library-hint-stash = Scene e immagini, dalle più vecchie. La chiave API è in Settings › Security. Stash non ha classificazioni di sicurezza, quindi ogni post riceve quella che scegli qui sotto. Gli interpreti diventano tag personaggio, gli studi tag artista, e un voto da 60 in su diventa un “mi piace”.
library-hint-blombooru = La chiave API del tuo account. Per alias e implicazioni serve la chiave di un amministratore di quel sito; le implicazioni basate su schemi di tag non arrivano.
library-hint-monbooru = Un token con accesso in lettura, se il sito li usa. Arrivano tutte le gallerie del server. I suoi alias non si possono leggere: Monbooru non ha un’API per questi.
library-hint-lanbooru = Nessun account: le letture di LANBooru sono aperte. Non ha alias, implicazioni o album.
library-unrated = Classificazione dei post senza un tag che la definisca
library-check = Controlla
library-checking = Controllo…
library-start = Importa tutto
library-found = Trovati { $count } post.
library-found-some = Il sito risponde, ma non dice quanti post ha.
library-no-rules = Alias e implicazioni resteranno fuori, perché qui solo un amministratore può importarli.
library-full = Possono attendere o essere in corso fino a cinque importazioni. Avvia questa quando un’altra finisce.
library-bad-address = Questo indirizzo non è valido. Scrivilo come http://192.168.1.4:8080.
library-need-login = Questo sito richiede il nome utente insieme alla chiave.
library-need-key = Questo sito richiede la chiave insieme al nome utente.
library-list = Importazioni
library-state-queued = In attesa
library-state-rules = Lettura di alias e implicazioni
library-state-posts = Importazione dei post
library-state-groups = Lettura di pool e “mi piace”
library-state-done = Finita
library-state-stopped = Fermata
library-state-failed = Non riuscita
library-count = { $done } fatti
library-count-of = { $done } di { $total }
library-count-merged = { $count } già presenti
library-count-failed = { $count } non riusciti
library-count-rules = { $count } alias e implicazioni
library-stop = Ferma
library-retry = Importa di nuovo
library-remove = Togli dall’elenco
library-show-failed = Mostra i non riusciti
library-account-again = La chiave è stata cancellata alla fine di questa importazione. Inseriscila di nuovo, o lascia vuoto per leggere ciò che vedono gli ospiti.
