<p align="center"><img src="assets/icon/kite-256.png" width="96" alt=""></p>

# Kite PDF

Lecteur PDF pour Windows écrit en Rust : rapide, économe en mémoire, avec annotations compatibles Adobe Acrobat et OCR des images intégrées.

## Installation

Téléchargez `KitePDF-Setup-x.y.z.exe` dans les [Releases](https://github.com/padimato/kite-pdf/releases) et lancez-le. L'installation se fait pour votre compte, sans droits administrateur. Elle ajoute Kite PDF au menu Démarrer et à « Ouvrir avec » pour les fichiers PDF ; une case de la dernière page ouvre les Paramètres de Windows pour en faire le lecteur par défaut.

Au premier lancement, l'application télécharge PDFium et l'OCR depuis leur source officielle (voir [Composants](#compilation)). L'exécutable n'est pas signé : Windows SmartScreen peut afficher « Windows a protégé votre ordinateur » ; cliquez sur « Informations complémentaires » puis « Exécuter quand même ».

Une seule fenêtre est ouverte à la fois : un PDF ouvert depuis l'Explorateur arrive dans un nouvel onglet. L'application signale discrètement les nouvelles versions publiées (une vérification par jour au plus, rien n'est installé sans vous) ; le bouton « Rechercher une mise à jour » des Réglages vérifie à la demande.

## Fonctionnalités

- **Onglets verticaux** dans une barre latérale épinglable ou masquée au survol (`Ctrl+B`). Les onglets en arrière-plan sont mis en veille pour libérer la mémoire.
- **Écran portrait** : quand la fenêtre est plus haute que large, la barre latérale laisse place à une barre d'une ligne en haut (onglets dans un menu, outils, page, zoom) et le sommaire s'ouvre en bas. Réglable dans les Réglages : automatique, toujours la barre latérale, ou toujours la barre du haut.
- **Barre centrale `Ctrl+T`** : rouvre un PDF de l'historique (recherche tolérante aux accents et aux abréviations), va à une page, ou trouve un outil en langage courant (« commentaire », « stabilo », « sauvegarder »). `Ctrl+Maj+T` rouvre le dernier onglet fermé.
- **Assistant IA** (`Ctrl+J`, ou le bouton rond en bas à droite) : questions sur les PDF ouverts, avec votre propre clé API (plusieurs fournisseurs en offrent une gratuite, voir [Assistant IA](#assistant-ia)). On choisit quels onglets servent de sources ; les réponses citent les pages (« p. 12 ») et un clic y mène. On peut joindre une zone de la page capturée à la souris (✂), une capture d'écran collée (`Ctrl+V`) ou une image glissée ; un clic droit sur un texte sélectionné propose « Expliquer », « Traduire » ou « Résumer ». Des boutons rapides préparent un résumé, les points clés, une fiche de révision ou un QCM.
- **Français ou anglais** : l'interface suit la langue de Windows, ou celle choisie dans les Réglages.
- **Annotations** : surlignage, texte libre, dessin, flèches et rectangles, notes avec fils de discussion. Elles sont enregistrées au format standard (ISO 32000) et relues par Acrobat.
- **Recherche `Ctrl+F`** dans le texte du document, y compris le texte des figures et des scans grâce à l'OCR.
- **OCR des images** (PaddleOCR PP-OCRv5, ONNX Runtime) en tâche de fond. Le texte reconnu est enregistré dans le PDF sous forme de couche invisible : il devient cherchable dans Acrobat, et les pages déjà analysées ne le sont plus à la réouverture.
- **Pavé tactile** : défilement fluide et zoom par pincement centré sur le pointeur.
- **Pour bien démarrer** : un écran d'accueil au premier lancement, l'aide-mémoire des raccourcis (`F1`) et une page de réglages (`Ctrl+,`) rangée en rubriques : Général (langue, disposition), Documents, Texte des images, Assistant IA, Confidentialité, Mises à jour, Aide.
- **Usage courant** : sommaire du document (signets), formulaires à remplir (texte, cases, boutons radio, listes), impression, PDF protégés par mot de passe, reprise de la lecture à la page et au zoom où on l'avait laissée.

## Assistant IA

L'assistant utilise votre propre clé API. Elle est rangée dans le Gestionnaire d'identification de Windows, jamais en clair sur le disque. Rien n'est envoyé avant que vous posiez une question ; les conversations restent sur l'ordinateur (on peut le désactiver).

| Fournisseur | Offre | Remarques | Testé |
|---|---|---|---|
| [Google Gemini](https://aistudio.google.com/apikey) | gratuite | Recommandé : lit les longs PDF en entier et les images ; premier mot en 1,3 à 2 s sur un article de 23 pages (`gemini-flash-lite-latest`) | oui (16/11/2026) |
| [NVIDIA NIM](https://build.nvidia.com/settings/api-keys) | crédits gratuits | `nemotron-3.5-lightning` sans raisonnement : premier mot en 1,6 s. D'autres modèles de la liste attendent parfois plus d'une minute leur tour | oui (16/11/2026) |
| [OpenRouter](https://openrouter.ai/settings/keys) | gratuite | Modèles « :free » souvent saturés (429) ; le modèle gratuit le plus fiable est choisi automatiquement (premier mot vers 8 s). À garder en relais | oui (16/11/2026) |
| [Groq](https://console.groq.com/keys) | gratuite | Le plus rapide d'après les retours, mais l'offre gratuite n'accepte que de petits textes : seules les pages proches de celle affichée sont envoyées | non |
| [Mistral](https://console.mistral.ai/api-keys) | gratuite | Hébergé en Europe, lit les longs PDF | non |
| [Cerebras](https://cloud.cerebras.ai/platform) | essai | Très rapide | non |
| Claude (Anthropic), OpenAI | payante | | non |
| Service compatible OpenAI | — | Ollama ou LM Studio sur l'ordinateur, ou toute adresse compatible | non |

**Relais** : si le fournisseur choisi ne répond pas (limite gratuite atteinte, panne, modèle retiré), la question part chez le suivant dont la clé est enregistrée, dans l'ordre Groq, Cerebras, Gemini, NVIDIA, Mistral, OpenRouter, puis les payants. Sous la réponse, l'assistant indique qui a répondu et le temps avant le premier mot. Enregistrer deux ou trois clés gratuites rend l'assistant bien plus fiable.

> **Appel aux retours.** Les fournisseurs marqués « non testé » n'ont pas pu être essayés par l'équipe avec une vraie clé : les requêtes suivent leur documentation et sont vérifiées par des tests automatiques sur un faux serveur, mais rien ne remplace un essai réel. Si vous en utilisez un, dites-nous si ça marche (ou ce qui coince) dans les [Issues](https://github.com/padimato/kite-pdf/issues). Kite PDF est open source : une pull request pour améliorer l'assistant ou ajouter un fournisseur est la bienvenue.

## Raccourcis

| Raccourci | Action |
|---|---|
| `Ctrl+T` / `Ctrl+K` | Barre centrale (PDF récents, outils, page ou zoom) |
| `Ctrl+Maj+T` | Rouvrir le dernier onglet fermé |
| `Ctrl+O` | Ouvrir un fichier (ou glisser-déposer un PDF dans la fenêtre) |
| `Ctrl+S` | Enregistrer (annotations, formulaire et texte OCR) |
| `Ctrl+Maj+S` | Enregistrer sous (l'original reste intact) |
| `Ctrl+P` | Imprimer |
| `Ctrl+F` | Rechercher dans le document |
| `Ctrl+W` / `Ctrl+Tab` | Fermer / changer d'onglet |
| `Ctrl+B` | Épingler ou masquer la barre latérale (la barre du haut du mode portrait reste affichée) |
| `Ctrl+9` / `Ctrl+0` | Ajuster à la largeur / à la page |
| `V` `H` `T` `D` `R` `N` `M` | Outils : sélection, surlignage, texte, dessin, flèche, note, mesure |
| `Ctrl+Z` / `Ctrl+Y` | Annuler / rétablir |
| `Ctrl+J` | Assistant IA |
| `Ctrl+,` | Réglages |
| `F1` | Aide-mémoire des raccourcis |

## Compilation

Prérequis : Windows 10 ou 11 (x64) et [Rust](https://rustup.rs) stable.

```bash
git clone https://github.com/padimato/kite-pdf.git
cd kite-pdf
cargo build --release
```

Les bibliothèques natives et les modèles OCR ne sont pas dans le dépôt : ils sont téléchargés depuis leur source officielle, dans la version listée dans [`deps.toml`](deps.toml) (adresse, version et empreinte SHA-256 de chaque fichier).

```bash
cargo run --release -- --fetch-deps
```

Cette commande installe dans le dossier courant les fichiers manquants :

| Fichier | Source |
|---|---|
| `pdfium.dll` | [binaires PDFium de bblanchon](https://github.com/bblanchon/pdfium-binaries/releases) |
| `assets/models/onnxruntime.dll` | [ONNX Runtime, Microsoft](https://github.com/microsoft/onnxruntime/releases) |
| `assets/models/*.onnx`, `ppocrv5_latin_dict.txt` | modèles PaddleOCR PP-OCRv5 au format ONNX, [publiés par oar-ocr](https://github.com/GreatV/oar-ocr/releases/tag/v0.3.0) |

Sans cette étape, l'application télécharge elle-même ces fichiers au premier lancement, dans `%LOCALAPPDATA%\KitePDF\deps` : l'écran d'installation attend PDFium, l'OCR peut être installé plus tard. La variable d'environnement `KITE_DEPS_DIR` désigne un autre dossier (installation portable).

**Autre version** : un fichier différent de la version testée est accepté. L'application affiche alors un avertissement qui indique la version testée et où la télécharger, en cas de problème. Pour adopter une nouvelle version, mettez à jour `deps.toml` puis lancez `cargo test`.

Sans les modèles, l'application fonctionne normalement ; seul l'OCR est désactivé.

## Lancement

```bash
cargo run --release -- chemin/vers/document.pdf
```

Sans argument, l'application s'ouvre sur la barre centrale et propose les PDF récents. L'historique est conservé dans `%APPDATA%\KitePDF\recent.tsv`.

## Tests

```bash
cargo test
```

Les tests créent leurs PDF dans `test_pdfs/` (ignoré par git). Les tests OCR demandent les dépendances ci-dessus (`--fetch-deps`).

## Outils de développement

| Commande ou variable | Rôle |
|---|---|
| `--fetch-deps [dossier]` | Installe PDFium et l'OCR (voir plus haut) |
| `--bench fichier.pdf` | Mesure le temps de chaque étape : ouverture, rendu, texte, OCR (à lancer en `--release`) |
| `KITE_TIMING=1` | Affiche le temps entre le lancement et la première page affichée, et la lecture du texte envoyé à l'assistant |
| `--lang fr\|en` | Langue de l'interface pour une capture |
| `--settings [rubrique]` | Ouvre les Réglages (`general`, `documents`, `ocr`, `assistant`, `privacy`, `updates`, `help`) |
| `--ai-setup`, `--ai-empty`, `--ai-demo`, `--ai-capture` | Assistant ouvert dans un état de démonstration (aucune requête envoyée) |
| `--ai-provider gemini\|groq\|…`, `--ai-model id` | Fournisseur et modèle de l'assistant pour un essai |
| `--ai-list-models fournisseur` | Liste les modèles accessibles avec la clé rangée (la clé n'est pas affichée) |
| `--ai-attach image.png` | Joint une image à la question de `--ai-ask` |
| `KITE_AI_DEBUG=1` | Journal des requêtes de l'assistant : adresse, statut, délais, début du flux (jamais la clé) |
| `KITE_USE_VAULT=1` | Avec `KITE_DATA_DIR`, lit quand même les clés rangées dans le coffre de Windows (essais avec de vraies clés) |
| `--ai-ask "question"` | Pose une question à l'assistant au démarrage (essai de bout en bout, par exemple avec un faux service local) |
| `--selection-menu` | Ouvre le menu du clic droit sur la sélection (avec `--select-stream`) |
| `KITE_DEPS_DIR` | Dossier des dépendances (installation portable, essais) |
| `KITE_DATA_DIR` | Dossier des réglages et de l'historique (installation portable, essais) |
| `KITE_QA=1` | Active dans une compilation `--release` les options de test (`--screenshot`, `--page`, `--search`…), toujours actives en compilation de développement |

## Publier une version

1. Mettre à jour `version` dans `Cargo.toml`, puis commiter.
2. `git tag v1.2.3` puis `git push origin v1.2.3`.

L'action GitHub [`release.yml`](.github/workflows/release.yml) compile, lance les tests, construit l'installateur avec [Inno Setup](https://jrsoftware.org/isinfo.php) (script [`installer/kite-pdf.iss`](installer/kite-pdf.iss)) et le publie dans les Releases. Pour le construire en local : `cargo build --release` puis `iscc installer\kite-pdf.iss` ; le résultat est dans `target\installer\`.

L'icône est générée par [`assets/icon/generate_icon.py`](assets/icon/generate_icon.py) (Pillow).

## Licence

Code publié sous licence [MIT](LICENSE).

Composants tiers, chacun sous sa propre licence : [PDFium](https://pdfium.googlesource.com/pdfium/) (BSD-3-Clause et Apache-2.0), [ONNX Runtime](https://github.com/microsoft/onnxruntime) (MIT), modèles [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) (Apache-2.0), ainsi que les crates listées dans `Cargo.toml`.
