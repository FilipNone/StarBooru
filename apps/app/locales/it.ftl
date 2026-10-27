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
search-placeholder = Cerca tag, ad es. blue_sky -cloud rating:g
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
settings-accent-custom = Tonalità personalizzata
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
password-current = Password attuale
password-new = Nuova password
password-too-short = Almeno { $count } caratteri.
password-repeat = Ripeti la nuova password
password-mismatch = Le password non coincidono.
password-changed = Password cambiata. Gli altri tuoi dispositivi sono stati disconnessi.
password-changing = Modifica in corso…
password-change = Cambia password
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

rating-general = generale
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
help-tag-spaces = due tag: gli spazi separano i termini, quindi scrivi blue_sky
help-filters = Filtri
help-rating = classificato generale (g), delicato (s), discutibile (q) o esplicito (e)
help-rating-any = una qualsiasi delle classificazioni elencate
help-rating-not = qualsiasi classificazione tranne esplicito
help-type = immagini statiche; anche type:animation e type:video
help-deleted = post nel cestino
help-sorting = Ordinamento
help-order-new = prima i più recenti (predefinito); order:old per i più vecchi
help-order-score = prima il punteggio più alto
help-order-size = prima i file più grandi
help-order-pixels = prima la risoluzione più alta
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
explain-order-tags-desc = Per numero di tag, prima i più numerosi
explain-order-tags-asc = Per numero di tag, prima i meno numerosi
explain-order-updated-desc = Per ultima modifica, prima la più recente
explain-order-updated-asc = Per ultima modifica, prima la meno recente

## Privacy Policy and Terms of Service

privacy-1 = StarBooru è un software che gira su un server scelto da te o da qualcuno di cui ti fidi. Niente di ciò che salvi viene inviato al progetto StarBooru o a chiunque altro.
privacy-2 = Chi è responsabile
privacy-3 = La persona o l'organizzazione che gestisce questo server (il gestore) decide chi può registrarsi ed è responsabile dei dati che contiene. Le domande sui tuoi dati vanno rivolte a lei.
privacy-4 = Cosa salva il server
privacy-5 = Il tuo account: nome utente, ruolo e un hash Argon2id con sale della tua password. La password in sé non viene mai salvata.
privacy-6 = Le tue sessioni: un hash di ogni token di accesso, il tipo di dispositivo da cui proviene (ad esempio “StarBooru app on Linux” o “Firefox on Android”), quando è stata creata e usata l'ultima volta, e quando scade. Le chiavi API che crei sono conservate allo stesso modo: un hash della chiave, il suo nome e quando è stata usata l'ultima volta.
privacy-7 = Ciò che aggiungi: i file caricati con dimensione, risoluzione, checksum e un'impronta visiva per la ricerca dei duplicati, oltre a tag, classificazioni, fonti, voti e ricerche salvate.
privacy-8 = Per rallentare i tentativi di indovinare le password, il server conta gli accessi falliti per account e per indirizzo di rete, e le registrazioni per indirizzo. Questi conteggi restano in memoria al massimo un'ora e non vengono mai scritti su disco.
privacy-9 = Cosa resta sul tuo dispositivo
privacy-10 = L'app ricorda l'indirizzo del server, il tuo token di accesso e le tue preferenze, come il tema. Uscendo il token viene rimosso.
privacy-11 = Cosa StarBooru non fa
privacy-12 = Niente statistiche d'uso, pubblicità o tracciamento, e nessun servizio di terze parti.
privacy-13 = Il server contatta altri siti solo quando importi da un URL. Quelle richieste usano un user agent generico e nessuno dei tuoi dati.
privacy-14 = Il contenuto dei file, le password e i token non vengono mai scritti nei log del server.
privacy-15 = Chi può vedere i tuoi post
privacy-16 = Chiunque abbia un account su questo server può vedere i post che contiene. I link ai file forniti all'app sono firmati e scadono, quindi un link copiato smette di funzionare entro 12 ore.
privacy-17 = Conservazione ed eliminazione
privacy-18 = I post eliminati finiscono nel cestino. Vengono cancellati quando un amministratore lo svuota, o automaticamente dopo il periodo stabilito dal gestore.
privacy-19 = Le sessioni terminano quando esci, quando scadono o quando disconnetti gli altri dispositivi dalle Impostazioni. Cambiare la password disconnette tutti gli altri dispositivi.
privacy-20 = Per eliminare il tuo account, chiedi a un amministratore di questo server.
privacy-21 = Il gestore può conservare dei backup, che seguono i suoi tempi.
terms-1 = Questi termini regolano l'uso di questo server StarBooru. Il gestore che lo amministra può aggiungerne altri.
terms-2 = Il tuo account
terms-3 = Non condividere la tua password e disconnetti i dispositivi che non usi più (Impostazioni › Dispositivi connessi).
terms-4 = Il primo account di un server ne è l'amministratore. Gli amministratori possono gestire tag, alias e implicazioni, e svuotare il cestino.
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
upload-or-choose = oppure fai clic per scegliere i file
upload-folder = Scegli una cartella
upload-folder-tags = Tagga i file con il nome della loro cartella
upload-urls-placeholder = Incolla indirizzi di immagini o video
upload-urls-label = Indirizzi di immagini o video
add = Aggiungi
upload-shared-tags = Tag per tutti i file
rating = Classificazione
upload-clear = Svuota l'elenco
upload-running = Caricamento…
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

## Tags

category-general = Generale
category-artist = Artista
category-character = Personaggio
category-copyright = Copyright
category-meta = Meta
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
post-upvote = Voto positivo
post-downvote = Voto negativo
post-download = Scarica
post-edit = Modifica
post-edit-tags = Modifica i tag
post-delete = Elimina
post-delete-title = Sposta nel cestino
post-tags = Tag
cancel = Annulla
save = Salva
post-no-tags = Ancora nessun tag.
post-details = Dettagli
post-size = Dimensione
post-dimensions = Risoluzione
post-length = Durata
post-type = Tipo
post-added = Aggiunto
post-source = Fonte
post-prev-title = Precedente (←)
post-prev = Post precedente
post-next-title = Successivo (→)
post-next = Post successivo
zoom-out-title = Riduci (−)
zoom-out = Riduci
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
gallery-stop-selecting = Termina la selezione
gallery-select = Seleziona i post
gallery-syntax = Sintassi di ricerca
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
sort-new = Più recenti
sort-old = Meno recenti
sort-score = Punteggio più alto
sort-updated = Modificati di recente
sort-size = File più grandi
sort-size-asc = File più piccoli
sort-pixels = Risoluzione più alta
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
