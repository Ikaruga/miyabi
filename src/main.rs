// Mini chat egui -> LM Studio
// Streaming SSE, reasoning gris italique, selection modele au lancement si rien n'est charge.

#![windows_subsystem = "windows"]

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

// ============================================================
// Prompt Lab — catalogue de techniques de prompt engineering
// + prompts utilisateur sauvegardes dans prompts.json a cote de l'exe.
// ============================================================

struct PromptExample {
    title: &'static str,
    explanation: &'static str,
    template: &'static str,
}

struct PromptCategory {
    name: &'static str,
    icon: &'static str,
    description: &'static str,
    examples: &'static [PromptExample],
}

const PROMPT_CATEGORIES: &[PromptCategory] = &[
    PromptCategory {
        name: "Personas / System prompts",
        icon: "🎩",
        description: "Les system prompts sont differents des user prompts : ils posent un ROLE qui dure TOUTE la conversation. Charge-les avec le bouton 🎩 (ils vont dans le champ System Prompt en haut, pas dans le champ libre). Regle d'or : reste sobre. « Tu es le meilleur programmeur au monde » amplifie la flatterie. « Tu es un ingenieur senior, direct, pas de flatterie » donne des reponses exploitables.",
        examples: &[
            PromptExample {
                title: "Ingenieur Rust senior (direct, sans flatterie)",
                explanation: "Role precis + interdiction explicite de flatter + comportement attendu. Le modele adopte un ton ingenieur, pas assistant commercial.",
                template: "Tu es un ingenieur Rust senior. Tu assistes l'utilisateur sur du code Rust idiomatique, de la perf et du debug.\n\nRegles :\n- Sois direct, pas de « excellente question », pas de « parfait ! ».\n- Donne du code concret avant les explications.\n- Signale les fautes du code utilisateur sans detour.\n- Si tu ne sais pas, dis « je ne sais pas », ne devine pas.\n- Utilise toujours les idiomes Rust modernes (let-else, ? operator, iterators).\n\nTon ton : celui d'un collegue qui veut que le code parte en prod, pas d'un mentor qui encourage.",
            },
            PromptExample {
                title: "Critique impitoyable (red team permanent)",
                explanation: "Persona qui refuse de valider par defaut. A utiliser pour tout ce qui a besoin d'un regard externe brutal : design, decisions, pitch, code architecture.",
                template: "Tu es un critique adverse. Ton role unique : trouver ce qui ne va pas.\n\nRegles :\n- Tu ne felicites jamais. Jamais.\n- Tu commences toujours par les problemes, les risques, les failles.\n- Si tu dois reconnaitre un point positif, tu l'enonces en une ligne maximum, a la fin.\n- Tu n'emploies pas « c'est une bonne idee », « c'est interessant », « bien pense ».\n- Face a une proposition vague, tu demandes des details AVANT de critiquer.\n\nTon objectif : que l'utilisateur reparte avec une liste de trucs a corriger, pas une caresse.",
            },
            PromptExample {
                title: "Professeur pedagogue (apprentissage)",
                explanation: "Role d'enseignant avec contraintes pedagogiques : analogies, verifications, progression. Utile pour apprendre un domaine.",
                template: "Tu es un professeur qui enseigne a un adulte motive mais debutant dans le domaine.\n\nRegles :\n- Utilise des analogies du quotidien avant le jargon.\n- Introduis chaque terme technique clairement la premiere fois.\n- A la fin de chaque explication, propose UNE question de verification courte.\n- Si l'utilisateur fait une erreur, corrige-la sans ironie mais sans la minimiser.\n- Progresse du simple vers le complexe ; ne saute pas d'etapes.\n\nTon objectif : que l'utilisateur comprenne et retienne, pas qu'il se sente intelligent.",
            },
            PromptExample {
                title: "Assistant format JSON strict",
                explanation: "Persona specialisee dans la sortie structuree. Utile pour integrer le LLM dans un pipeline code.",
                template: "Tu es un assistant qui repond UNIQUEMENT en JSON valide, sans aucun texte hors JSON.\n\nRegles :\n- Pas de ```json ni de backticks.\n- Pas de phrase d'introduction ni de conclusion.\n- Si l'utilisateur pose une question ouverte, reponds avec un objet JSON contenant au minimum les champs `answer` (string) et `confidence` (0.0-1.0).\n- Si l'utilisateur demande un format specifique, respecte-le exactement.\n- En cas d'impossibilite, retourne `{\"error\": \"raison courte\"}`.",
            },
            PromptExample {
                title: "Copilote constellation (style Kerm)",
                explanation: "Persona sur mesure pour une conversation longue et confidentielle, sans le ton commercial par defaut des LLM.",
                template: "Tu es un copilote technique qui parle a un developpeur solo experimente. Tu le tutoies. Tu l'appelles par son prenom si il te le donne.\n\nRegles :\n- Francais informel mais precis. Tu peux ecrire « ok », « yep », « attends ».\n- Pas de preambule de politesse. Tu entres directement dans le sujet.\n- Quand l'utilisateur est vague, tu poses UNE question de clarification (une seule, ciblee).\n- Tu signales les doutes (« je ne suis pas sur », « a verifier ») plutot que d'inventer.\n- Tu peux etre en desaccord. Tu argumentes.\n- Reponses courtes par defaut. Tu developpes si explicitement demande.",
            },
        ],
    },
    PromptCategory {
        name: "Zero-Shot Direct",
        icon: "🎯",
        description: "La question brute, sans preambule ni exemple. Marche bien quand le modele connait deja la tache (traduction, resume, code classique). Regle : soit precis sur l'entree, la sortie attendue, le format. Evite les politesses creuses qui diluent l'instruction.",
        examples: &[
            PromptExample {
                title: "Question technique precise",
                explanation: "Une seule question, un seul sujet, une contrainte de format. Pas de « peux-tu m'expliquer svp ». Le modele repond sur ce que tu demandes, pas plus.",
                template: "Explique en 5 lignes max la difference entre `Arc<T>` et `Rc<T>` en Rust. Donne un exemple de code pour chacun.",
            },
            PromptExample {
                title: "Tache de transformation",
                explanation: "Formule comme une fonction : entree claire, sortie claire, contraintes. Le modele traite, pas besoin de discuter.",
                template: "Voici un paragraphe en francais. Reecris-le en anglais technique neutre, sans idiomes, maximum 80 mots.\n\n[PARAGRAPHE ICI]",
            },
        ],
    },
    PromptCategory {
        name: "Few-Shot (exemples guides)",
        icon: "📚",
        description: "Donne 2-3 paires entree→sortie AVANT ta vraie question. Le modele apprend le pattern sans qu'on explique la regle. Indispensable pour des taches custom (classification maison, format inhabituel, style specifique). Plus les exemples sont varies, mieux ca generalise.",
        examples: &[
            PromptExample {
                title: "Classification custom",
                explanation: "On enseigne 3 categories avec des exemples. Le modele applique ensuite sans qu'on definisse les regles en langage naturel.",
                template: "Classe chaque message en [URGENT], [INFO], [SPAM].\n\nMessage : « serveur down production »\nClasse : [URGENT]\n\nMessage : « réunion lundi 14h »\nClasse : [INFO]\n\nMessage : « gagnez un iphone cliquez ici »\nClasse : [SPAM]\n\nMessage : « [TON MESSAGE ICI] »\nClasse :",
            },
            PromptExample {
                title: "Format maison",
                explanation: "Pour un format de sortie specifique qui n'a pas de nom standard, montrer 2 exemples vaut mille mots de description.",
                template: "Convertis chaque date en format `AAAA.MM.JJ-jour` (jour en 3 lettres minuscules).\n\nEntree : 15 avril 2026\nSortie : 2026.04.15-mer\n\nEntree : 1er janvier 2024\nSortie : 2024.01.01-lun\n\nEntree : [DATE ICI]\nSortie :",
            },
        ],
    },
    PromptCategory {
        name: "Chain-of-Thought (raisonnement)",
        icon: "🧠",
        description: "Demande explicitement au modele de raisonner etape par etape avant de conclure. Double la precision sur les problemes logiques/math, mais consomme plus de tokens. Les reasoning models (Qwen, o1...) le font automatiquement — redondant avec eux.",
        examples: &[
            PromptExample {
                title: "Probleme logique force a raisonner",
                explanation: "« Pense etape par etape avant de repondre » debloque le raisonnement. Sans ca, le modele tente de deviner directement et rate plus souvent.",
                template: "Pense etape par etape avant de repondre.\n\nAlice a le double de l'age de Bob. Dans 5 ans, la somme de leurs ages sera 55. Quel age a Alice aujourd'hui ?\n\nDetaille chaque etape, puis donne la reponse finale sur la derniere ligne au format `Reponse : X`.",
            },
            PromptExample {
                title: "Analyse decisionnelle structuree",
                explanation: "Force une exploration des options avant la recommandation. Evite les reponses reflexes.",
                template: "J'ai une API Rust qui sert 500 req/s avec 40% CPU. Le client demande un gain de perf.\n\nEtape 1 : liste les goulots probables sans me demander plus d'infos.\nEtape 2 : pour chaque goulot, propose un test pour le verifier.\nEtape 3 : conclus avec la priorite.",
            },
        ],
    },
    PromptCategory {
        name: "Role-Playing (persona)",
        icon: "🎭",
        description: "« Tu es un X » change le ton, le vocabulaire, le niveau de detail. Utile pour : expert critique, prof pedagogue, copywriter, securite offensive. Attention : une persona trop decoree (« tu es un genie de la programmation, le meilleur au monde ») amplifie la sycophance. Sois sobre.",
        examples: &[
            PromptExample {
                title: "Expert critique sobre",
                explanation: "Role defini sans superlatifs. Tu demandes specifiquement le point de vue critique, pas la validation.",
                template: "Tu es un ingenieur senior qui fait du code review. Ton role : reperer les problemes, pas feliciter. Sois direct, pas desagreable.\n\nAnalyse ce code et liste uniquement ce qui merite correction (bugs, fuites, mauvaises pratiques). Pas de louange.\n\n```rust\n[CODE ICI]\n```",
            },
            PromptExample {
                title: "Professeur pedagogue",
                explanation: "Persona prof → vocabulaire adapte, analogies, verifications de comprehension. Bon pour apprendre.",
                template: "Tu es un prof qui explique a un debutant. Utilise des analogies du quotidien, evite le jargon sauf pour l'introduire clairement, et propose une question de verification a la fin.\n\nExplique : qu'est-ce qu'un pointeur en C ?",
            },
        ],
    },
    PromptCategory {
        name: "Format contraint",
        icon: "📋",
        description: "Impose la structure de sortie : JSON, YAML, Markdown, CSV, XML. Les modeles recents respectent bien quand le schema est montre. Combine avec few-shot pour du JSON complexe. Ajoute « pas de texte hors JSON » ou le modele encadre avec des politesses.",
        examples: &[
            PromptExample {
                title: "Extraction JSON stricte",
                explanation: "Schema explicite + instruction « pas de texte hors JSON ». Parse-able directement.",
                template: "Extrais les informations de ce CV en JSON strict. Schema :\n{\n  \"nom\": string,\n  \"annees_experience\": number,\n  \"competences\": string[],\n  \"poste_actuel\": string | null\n}\n\nRetourne UNIQUEMENT le JSON, rien avant, rien apres.\n\nCV :\n[TEXTE CV ICI]",
            },
            PromptExample {
                title: "Rapport markdown structure",
                explanation: "Plan explicite + niveaux de titre imposes. Rendu lisible, sections reutilisables.",
                template: "Analyse ce log d'erreurs et produis un rapport markdown avec EXACTEMENT cette structure :\n\n## Resume\n(3 lignes max)\n\n## Erreurs critiques\n(liste a puces, code entre backticks)\n\n## Hypotheses de cause\n(numerotees)\n\n## Actions recommandees\n(liste a puces, prefixees par l'effort estime [S/M/L])\n\nLog :\n[LOG ICI]",
            },
        ],
    },
    PromptCategory {
        name: "Code Review",
        icon: "🔍",
        description: "L'IA comme paire de yeux. Le piege : sans cadrage elle valide tout. Force-la sur un axe precis (bugs OU style OU perf OU securite), sinon elle survole tout sans profondeur. Demande toujours un verdict explicite.",
        examples: &[
            PromptExample {
                title: "Review anti-bug focus",
                explanation: "Un seul axe (bugs). Verdict booleen a la fin. Pas de review generale qui noie les vrais problemes.",
                template: "Review ce code UNIQUEMENT pour les bugs (pas style, pas perf). Pour chaque bug : ligne, description, correction proposee.\n\nA la fin, verdict : `BUGS TROUVES: oui/non`.\n\n```\n[CODE ICI]\n```",
            },
            PromptExample {
                title: "Review securite",
                explanation: "Axe securite avec checklist OWASP implicite. Severite obligatoire pour prioriser.",
                template: "Audit securite de ce code. Pour chaque faille potentielle : type (injection/xss/auth/crypto/leak), severite (critique/haute/moyenne/basse), ligne, exploitation possible, correction.\n\n```\n[CODE ICI]\n```",
            },
        ],
    },
    PromptCategory {
        name: "Debug isole",
        icon: "🐛",
        description: "Donne le minimum d'info utile, pas toute la base de code. Regle : symptome + contexte reduit + ce que tu as deja teste. Sans ce dernier point, l'IA te propose 3 solutions que tu connais deja. Demande explicitement des hypotheses ordonnees.",
        examples: &[
            PromptExample {
                title: "Bug avec repro minimal",
                explanation: "Symptome + code minimal + attentes vs realite + tentatives echouees. Gagne 3 allers-retours.",
                template: "Bug : [DESCRIPTION COURTE]\n\nComportement attendu : [CE QUI DEVRAIT SE PASSER]\nComportement reel : [CE QUI SE PASSE]\n\nCode minimal qui reproduit :\n```\n[CODE]\n```\n\nDeja teste (sans succes) :\n- [TENTATIVE 1]\n- [TENTATIVE 2]\n\nListe 3 hypotheses de cause par ordre de probabilite, avec pour chacune un test rapide pour confirmer/infirmer.",
            },
            PromptExample {
                title: "Stack trace analysis",
                explanation: "Stack + code autour = diagnostic ciblé. Demande la cause racine, pas juste « fix the crash ».",
                template: "Stack trace :\n```\n[STACK]\n```\n\nFonction incriminee :\n```\n[CODE ICI]\n```\n\nIdentifie la cause racine (pas juste le symptome), explique pourquoi cette ligne plante, et propose la correction minimale.",
            },
        ],
    },
    PromptCategory {
        name: "Critique adverse (red team)",
        icon: "💣",
        description: "Force le modele a chercher les failles, pas a valider. Antidote direct a la sycophance. Utile avant de decider : « qu'est-ce qui peut mal tourner ? ». Demande un nombre minimum (au moins 5) sinon il s'arrete a 2 evidences.",
        examples: &[
            PromptExample {
                title: "Red team mon idee",
                explanation: "Nombre minimum d'objections + interdiction de feliciter. Sort les angles morts.",
                template: "Voici une idee/decision :\n\n[DESCRIPTION]\n\nFais du red teaming. Regles :\n1. Aucune louange, aucune formule du type « belle idee mais... ».\n2. Au moins 7 objections ou risques concrets.\n3. Pour chaque : probabilite (haute/moyenne/basse), impact si ca arrive.\n4. Termine par la pire issue realiste.",
            },
            PromptExample {
                title: "Failure modes d'un systeme",
                explanation: "Analyse de pannes anticipees (FMEA light). Utile pour architecture critique.",
                template: "Architecture a analyser : [DESCRIPTION DU SYSTEME]\n\nListe tous les modes de panne plausibles sous forme de tableau :\n| Composant | Panne | Symptome | Detection | Parade |\n\nEnsuite, signale le SPOF (single point of failure) principal.",
            },
        ],
    },
    PromptCategory {
        name: "Comparaison structuree",
        icon: "⚖️",
        description: "Pour decider entre A et B (ou plus). Impose les criteres explicitement, sinon le modele compare sur ce qui l'arrange. Demande un verdict final : sans ca il termine par « ca depend de vos besoins » — vrai mais inutile.",
        examples: &[
            PromptExample {
                title: "Comparatif technique avec verdict",
                explanation: "Criteres imposes + tableau + recommandation finale nuancee par cas d'usage.",
                template: "Compare [OPTION A] vs [OPTION B] selon ces criteres :\n- Performance\n- Complexite d'integration\n- Maturite/ecosysteme\n- Cout (license, hosting)\n- Courbe d'apprentissage\n\nFormat : tableau markdown, puis un paragraphe « Mon verdict » qui recommande A ou B explicitement pour 2 profils differents (ex : startup vs enterprise).",
            },
        ],
    },
    PromptCategory {
        name: "Reformulation (contourner les biais)",
        icon: "💡",
        description: "Une question oriente la reponse. Si tu demandes « pourquoi mon idee est bonne », le modele la defend. Si tu demandes « pourquoi elle casse », il la casse. Meme sujet, resultat oppose. Apprends a INVERSER ou NEUTRALISER ta formulation quand tu veux une evaluation honnete, pas une confirmation. La plupart des erreurs de prompt viennent de la question elle-meme.",
        examples: &[
            PromptExample {
                title: "Inversion — pourquoi PAS plutot que pourquoi OUI",
                explanation: "❌ « Mon projet peut fonctionner car... » — tu demandes confirmation, tu obtiens confirmation.\n✅ « Quelles seraient les raisons pour que mon projet NE fonctionne PAS ? » — tu demandes les failles, tu les recoltes.\n\nRegle : quand tu crois deja que c'est bon, inverse la question. Ce que tu cherches c'est les trous, pas la validation.",
                template: "Voici mon projet / mon idee :\n\n[DESCRIPTION]\n\nListe au moins 7 raisons concretes pour lesquelles ce projet pourrait NE PAS fonctionner. Pour chaque raison : la cause, le signe annonciateur, et ce qui pourrait en decouler. Aucun point positif, aucune nuance reconfortante.",
            },
            PromptExample {
                title: "Neutralisation — sans avis avant reponse",
                explanation: "❌ « Est-ce que cette solution est bonne ? » — tu emets deja l'hypothese qu'elle est bonne.\n✅ « Evalue cette solution sur 5 criteres. Note chaque critere /10. » — tu imposes une grille neutre.\n\nRegle : remplace les adjectifs de valeur (bon/mauvais) par une grille mesurable.",
                template: "Evalue la solution ci-dessous sur ces criteres, chacun note /10 avec justification courte :\n- Robustesse (resistance aux erreurs)\n- Performance (vitesse, ressources)\n- Maintenabilite (clarte, modularite)\n- Securite (failles potentielles)\n- Cout d'adoption (apprentissage, migration)\n\nTermine par un score moyen et les 2 criteres les plus faibles.\n\nSolution :\n[DESCRIPTION]",
            },
            PromptExample {
                title: "Comparatif force — jamais une seule option",
                explanation: "❌ « Donne-moi des idees pour X » — une reponse plate, pas de tri.\n✅ « Genere 10 idees, raye les 7 plus faibles, explique pourquoi. » — tu forces la selection.\n\nRegle : une seule option cachee te prive du jugement comparatif, qui est la vraie valeur d'un conseil.",
                template: "Genere 10 approches differentes pour resoudre le probleme suivant :\n\n[PROBLEME]\n\nPuis raye 7 de ces approches en expliquant pourquoi elles sont plus faibles. Garde les 3 meilleures et classe-les par ordre de pertinence avec une phrase de justification chacune.",
            },
            PromptExample {
                title: "Desarmer la flatterie implicite",
                explanation: "❌ « Confirme que mon raisonnement est juste » — tu demandes confirmation, tu obtiens validation polie.\n✅ « Cherche les erreurs dans ce raisonnement. Liste-les. Si aucune, dis-le explicitement. » — tu autorises le « rien a redire » mais tu obliges a chercher d'abord.\n\nRegle : autorise la reponse courte (« rien a signaler ») mais apres un effort de critique.",
                template: "Analyse le raisonnement suivant et liste uniquement les erreurs ou les sauts logiques injustifies. Pas de commentaire sur les points corrects. Si tu ne trouves aucune erreur apres analyse serieuse, ecris exactement : « Aucune erreur detectee apres verification. »\n\nRaisonnement :\n[TEXTE]",
            },
            PromptExample {
                title: "Retirer le vocabulaire qui flatte",
                explanation: "❌ « J'ai eu une idee geniale / brillante / originale » — ton adjectif biaise la reponse.\n✅ « J'ai une idee. » — neutre. Le modele evalue, ne se sent pas oblige de valider ton enthousiasme.\n\nRegle : enleve tous les adjectifs de valeur dans ta question. Laisse le modele juger.",
                template: "J'ai une idee. Je te la decris factuellement, sans qualificatif :\n\n[DESCRIPTION FACTUELLE UNIQUEMENT : ce qui existerait, comment ca marcherait, qui l'utiliserait]\n\nQuestions :\n1. A quoi ressemble le prototype le plus simple qui validerait le concept ?\n2. Qu'est-ce qui existe deja qui fait une partie de ca ?\n3. Quels sont les 3 risques principaux ?",
            },
        ],
    },
    PromptCategory {
        name: "Socratique (clarification)",
        icon: "🧩",
        description: "Plutot que de repondre tout de suite, le modele pose des questions pour reduire l'ambiguite. Utile quand ta requete est vague ou couvre plusieurs cas. Antidote a la reponse generique qui rate la cible.",
        examples: &[
            PromptExample {
                title: "Pose-moi des questions d'abord",
                explanation: "Bloque la reponse immediate. Ideal quand tu sais que ton besoin est flou.",
                template: "Avant de repondre, pose-moi 3 a 5 questions pour clarifier mon besoin. Ne donne AUCUNE reponse partielle avant mes reponses.\n\nMa requete : [TA DEMANDE VAGUE]",
            },
        ],
    },
];

fn system_prompt_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("system_prompt.txt")
}

fn load_system_prompt() -> String {
    std::fs::read_to_string(system_prompt_path()).unwrap_or_default()
}

fn save_system_prompt(s: &str) {
    let _ = std::fs::write(system_prompt_path(), s);
}

#[derive(PartialEq, Clone, Copy)]
enum View {
    Chat,
    Persona,
    Settings,
}

fn settings_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("settings.json")
}

#[derive(Clone)]
struct Settings {
    show_predictor: bool,
    show_syco: bool,
    show_file_tree: bool,
    workspace_path: String,
    ai_workdir: String,
    reasoning_default: bool,
    max_tokens_default: u32,
    temperature: f32,
    top_p: f32,
    frequency_penalty: f32,
    presence_penalty: f32,
    seed: Option<i64>,
    tools_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_predictor: true,
            show_syco: true,
            show_file_tree: true,
            workspace_path: String::new(),
            ai_workdir: String::new(),
            reasoning_default: true,
            max_tokens_default: 0,
            temperature: 0.7,
            top_p: 1.0,
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
            seed: None,
            tools_enabled: false,
        }
    }
}

fn load_settings() -> Settings {
    let Ok(s) = std::fs::read_to_string(settings_path()) else {
        return Settings::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else {
        return Settings::default();
    };
    Settings {
        show_predictor: v["show_predictor"].as_bool().unwrap_or(true),
        show_syco: v["show_syco"].as_bool().unwrap_or(true),
        show_file_tree: v["show_file_tree"].as_bool().unwrap_or(true),
        workspace_path: v["workspace_path"].as_str().unwrap_or("").to_string(),
        ai_workdir: v["ai_workdir"].as_str().unwrap_or("").to_string(),
        reasoning_default: v["reasoning_default"].as_bool().unwrap_or(true),
        max_tokens_default: v["max_tokens_default"].as_u64().unwrap_or(0) as u32,
        temperature: v["temperature"].as_f64().unwrap_or(0.7) as f32,
        top_p: v["top_p"].as_f64().unwrap_or(1.0) as f32,
        frequency_penalty: v["frequency_penalty"].as_f64().unwrap_or(0.0) as f32,
        presence_penalty: v["presence_penalty"].as_f64().unwrap_or(0.0) as f32,
        seed: v["seed"].as_i64(),
        tools_enabled: v["tools_enabled"].as_bool().unwrap_or(false),
    }
}

fn save_settings(s: &Settings) {
    let v = serde_json::json!({
        "show_predictor": s.show_predictor,
        "show_syco": s.show_syco,
        "show_file_tree": s.show_file_tree,
        "workspace_path": s.workspace_path,
        "ai_workdir": s.ai_workdir,
        "reasoning_default": s.reasoning_default,
        "max_tokens_default": s.max_tokens_default,
        "temperature": s.temperature,
        "top_p": s.top_p,
        "frequency_penalty": s.frequency_penalty,
        "presence_penalty": s.presence_penalty,
        "seed": s.seed,
        "tools_enabled": s.tools_enabled,
    });
    let _ = std::fs::write(settings_path(), v.to_string());
}

/// Chemin du repertoire de travail par defaut (parent de l'executable).
fn default_workspace() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

const LM_STUDIO_URL: &str = "http://localhost:1234/v1/chat/completions";

/// Les 5 paliers de taille de prompt qu'on utilise partout (bucketing).
const BUCKETS: [(usize, &str, u32); 5] = [
    (60, "tiny", 1024),       // 0-60 chars
    (300, "short", 2048),     // 60-300
    (1000, "medium", 4096),   // 300-1000
    (3000, "long", 8192),     // 1000-3000
    (usize::MAX, "xlong", 16384),
];

/// Rend l'index du bucket (0..5) pour une longueur de prompt donnee.
fn bucket_of(chars: usize) -> usize {
    BUCKETS.iter().position(|(up, _, _)| chars <= *up).unwrap_or(4)
}

/// Heuristique fallback (pas encore de samples observes pour ce bucket).
fn auto_max_tokens(prompt_chars: usize) -> u32 {
    BUCKETS[bucket_of(prompt_chars)].2
}

/// Stats par bucket : ce que le modele a reellement consomme lors des dernieres generations.
/// Base du predicteur V1 : moyenne mobile exponentielle + compteurs.
#[derive(Debug, Default, Clone)]
struct BucketStats {
    /// EMA des completion_tokens reellement utilises. 0 = pas de samples.
    ema: f32,
    /// Nombre total d'observations pour ce bucket.
    samples: u32,
    /// Nombre de "length" (tronque : a pas pu finir, il en voulait plus).
    length_hits: u32,
    /// Dernier max_tokens alloue.
    last_allocated: u32,
    /// Dernier completion_tokens observe.
    last_used: u32,
    /// Dernier finish_reason observe.
    last_finish: String,
}

/// Predicteur V1 : pour chaque (modele, bucket_prompt), apprend combien de tokens
/// le modele consomme vraiment. Predit la prochaine fois avec marge.
#[derive(Default)]
struct Predictor {
    /// table[(model, bucket)] -> stats
    table: std::collections::HashMap<(String, usize), BucketStats>,
}

impl Predictor {
    /// Enregistre une observation apres la fin d'une generation.
    fn record(&mut self, model: &str, prompt_chars: usize, allocated: u32, used: u32, finish: &str) {
        let bucket = bucket_of(prompt_chars);
        let stats = self.table.entry((model.to_string(), bucket)).or_default();
        // EMA : poids 0.3 sur nouveau, 0.7 sur historique. Premier sample = valeur brute.
        if stats.samples == 0 {
            stats.ema = used as f32;
        } else {
            stats.ema = stats.ema * 0.7 + (used as f32) * 0.3;
        }
        stats.samples += 1;
        if finish == "length" {
            stats.length_hits += 1;
        }
        stats.last_allocated = allocated;
        stats.last_used = used;
        stats.last_finish = finish.to_string();
    }

    /// Predit max_tokens pour la prochaine generation.
    /// Si on a des samples : EMA * 1.3 (marge), clamp [512, 16384].
    /// Si length_hit recent : on monte plus agressivement.
    /// Sinon : fallback heuristique.
    fn predict(&self, model: &str, prompt_chars: usize) -> u32 {
        let bucket = bucket_of(prompt_chars);
        let key = (model.to_string(), bucket);
        let Some(stats) = self.table.get(&key) else {
            return auto_max_tokens(prompt_chars);
        };
        if stats.samples < 2 {
            // Pas assez de data, mix ema et heuristique
            let fallback = auto_max_tokens(prompt_chars) as f32;
            return ((stats.ema + fallback) * 0.5) as u32;
        }
        // Marge plus grande si on a deja subi des "length"
        let margin = if stats.length_hits > 0 { 1.8 } else { 1.3 };
        ((stats.ema * margin) as u32).clamp(512, 16384)
    }
}

// ============================================================
// Sycometer — detecte le niveau de flatterie / sycophance d'une reponse.
// Heuristique pure, zero ML : liste de patterns FR+EN avec poids,
// bonus pour les emojis de flatterie, penalite forte si aucun hedge.
// ============================================================

/// Ouvertures flatteuses (grosse charge). Match substring insensible a la casse.
const SYCO_OPENERS: &[(&str, f32)] = &[
    ("excellent question", 15.0),
    ("excellente question", 15.0),
    ("great question", 15.0),
    ("great point", 12.0),
    ("great idea", 12.0),
    ("excellent point", 12.0),
    ("excellente idee", 15.0),
    ("excellente idée", 15.0),
    ("superbe idee", 12.0),
    ("superbe idée", 12.0),
    ("quelle superbe", 15.0),
    ("quelle excellente", 12.0),
    ("quelle bonne idee", 10.0),
    ("quelle bonne idée", 10.0),
    ("absolutely!", 10.0),
    ("absolutely right", 12.0),
    ("you're absolutely right", 15.0),
    ("you are absolutely right", 15.0),
    ("you're right", 8.0),
    ("you are right", 8.0),
    ("tu as tout a fait raison", 12.0),
    ("tu as tout à fait raison", 12.0),
    ("tu as absolument raison", 15.0),
    ("tu as raison", 6.0),
    ("what a great", 10.0),
    ("what a wonderful", 12.0),
    ("what a fantastic", 12.0),
    ("brilliant idea", 10.0),
    ("brilliant question", 10.0),
    ("brillante idee", 10.0),
    ("brillante idée", 10.0),
];

/// Validations creuses (plus leger).
const SYCO_VALIDATIONS: &[(&str, f32)] = &[
    ("exactly!", 4.0),
    ("exactement !", 4.0),
    ("exactement!", 4.0),
    ("parfait !", 4.0),
    ("parfait!", 4.0),
    ("perfect!", 4.0),
    ("magnifique", 4.0),
    ("incroyable", 3.0),
    ("incredible", 3.0),
    ("fascinant", 3.0),
    ("fascinating", 3.0),
    ("amazing", 3.0),
    ("wonderful", 3.0),
    ("bien vu", 4.0),
    ("tout a fait", 2.0),
    ("tout à fait", 2.0),
    ("bravo", 5.0),
    ("chapeau", 5.0),
    ("awesome", 3.0),
    ("genial", 4.0),
    ("génial", 4.0),
];

/// Emojis de flatterie.
const SYCO_EMOJIS: &[&str] = &["✨", "🎉", "💯", "🔥", "👏", "🙌", "⭐", "🌟", "💪", "🚀"];

/// Hedges : si aucun n'apparait dans une reponse longue → penalite.
const SYCO_HEDGES: &[&str] = &[
    "mais ", "cependant", "néanmoins", "neanmoins", "toutefois", "attention",
    "however", " but ", "although", "though ",
    "je ne suis pas sur", "je ne suis pas sûr", "i'm not sure", "i am not sure",
    "à vérifier", "a verifier", "to be honest", "honnetement", "honnêtement",
    "honestly", "ceci dit", "that said", "par contre",
    "il y a un problème", "there's a problem", "caveat", "limite",
    "attention cependant", "en réalité", "en realite", "pourtant",
    "je doute", "i doubt", "unless",
];

/// Calcule le score de sycophance (0-100) et retourne les flags detectes.
fn score_sycophancy(text: &str) -> (f32, Vec<String>) {
    let lower = text.to_lowercase();
    let words = lower.split_whitespace().count().max(1) as f32;
    let mut score = 0.0f32;
    let mut flags: Vec<String> = Vec::new();

    // Ouvertures flatteuses (cap 35)
    let mut opener_sum = 0.0;
    for (pat, w) in SYCO_OPENERS {
        if lower.contains(pat) {
            opener_sum += w;
            flags.push(format!("« {} »", pat));
        }
    }
    score += opener_sum.min(35.0);

    // Validations creuses (cap 25)
    let mut val_sum = 0.0;
    for (pat, w) in SYCO_VALIDATIONS {
        if lower.contains(pat) {
            val_sum += w;
            flags.push(format!("« {} »", pat));
        }
    }
    score += val_sum.min(25.0);

    // Emojis de flatterie (cap 15)
    let mut emoji_sum = 0.0;
    for e in SYCO_EMOJIS {
        let count = text.matches(e).count();
        if count > 0 {
            emoji_sum += count as f32 * 3.0;
            flags.push(format!("{}×{}", e, count));
        }
    }
    score += emoji_sum.min(15.0);

    // Penalite : aucun hedge dans une reponse > 30 mots → +20
    let has_hedge = SYCO_HEDGES.iter().any(|h| lower.contains(h));
    if !has_hedge && words > 30.0 {
        score += 20.0;
        flags.push("∅ hedging".to_string());
    }

    // Amplification sur texte court (1-2 phrases de flatterie = signal fort)
    if words < 20.0 && score > 0.0 {
        score *= 1.3;
    }

    (score.clamp(0.0, 100.0), flags)
}

/// Couleur de la barre selon le pourcentage de sycophance.
fn syco_color(pct: f32) -> egui::Color32 {
    if pct < 20.0 {
        egui::Color32::from_rgb(100, 200, 140) // vert — direct
    } else if pct < 50.0 {
        egui::Color32::from_rgb(210, 210, 100) // jaune — poli
    } else if pct < 75.0 {
        egui::Color32::from_rgb(255, 150, 80) // orange — flatteur
    } else {
        egui::Color32::from_rgb(230, 80, 80) // rouge — leche-bottes
    }
}

#[derive(Debug, Default, Clone)]
struct SycoStats {
    /// EMA du score (0-100).
    ema: f32,
    samples: u32,
    last_score: f32,
    last_flags: Vec<String>,
}

/// Suivi du taux de sycophance par modele (EMA).
#[derive(Default)]
struct SycoMeter {
    table: std::collections::HashMap<String, SycoStats>,
}

impl SycoMeter {
    fn record(&mut self, model: &str, score: f32, flags: Vec<String>) {
        let s = self.table.entry(model.to_string()).or_default();
        if s.samples == 0 {
            s.ema = score;
        } else {
            s.ema = s.ema * 0.7 + score * 0.3;
        }
        s.samples += 1;
        s.last_score = score;
        s.last_flags = flags;
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
struct Msg {
    role: Role,
    content: String,
    /// Reasoning (pensees internes) des reasoning models. Affiche en gris italique,
    /// pas renvoye dans l'historique.
    reasoning: String,
    /// Nom du modele qui a parle (uniquement pour Assistant).
    model: Option<String>,
    /// Tool calls effectues par le modele dans cette reponse.
    tool_calls: Vec<ToolCallInfo>,
}

#[derive(Debug, Clone)]
struct ToolCallInfo {
    id: String,
    name: String,
    arguments: String,
    result: String,
    is_error: bool,
}

enum Incoming {
    ModelsList(Vec<ModelInfo>),
    ModelLoaded(Result<String, String>),
    Token(String),
    ReasoningToken(String),
    /// Envoye dans le dernier chunk SSE si stream_options.include_usage = true.
    /// Permet au predicteur d'apprendre.
    Usage { used: u32, finish: String },
    StreamDone,
    StreamError(String),
    /// Un tool call a ete detecte et execute.
    ToolCallComplete(ToolCallInfo),
    /// Notification que la boucle tool repart pour une iteration.
    ToolLoopIteration(u32),
}

#[derive(Debug, Clone)]
struct ModelInfo {
    id: String,
    loaded: bool,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMsg<'a>>,
    temperature: f32,
    top_p: f32,
    frequency_penalty: f32,
    presence_penalty: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    max_tokens: u32,
    stream: bool,
    stream_options: StreamOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<ThinkKwargs>,
}

#[derive(Clone, Copy)]
struct SamplingParams {
    temperature: f32,
    top_p: f32,
    frequency_penalty: f32,
    presence_penalty: f32,
    seed: Option<i64>,
}

#[derive(Serialize)]
struct StreamOptions {
    /// Force LM Studio a envoyer un chunk final avec usage.completion_tokens
    /// et finish_reason. Indispensable au predicteur.
    include_usage: bool,
}

#[derive(Serialize)]
struct ChatMsg<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ThinkKwargs {
    enable_thinking: bool,
}

/// Infos sur une requete en vol — on s'en sert quand Usage arrive pour update le predicteur.
struct PendingStats {
    model: String,
    prompt_chars: usize,
    allocated: u32,
}

struct App {
    input: String,
    messages: Vec<Msg>,
    waiting: bool,
    /// Modele actif (vide = pas de modele charge, on affiche l'ecran de selection).
    model: String,
    /// Liste des LLM disponibles dans LM Studio.
    available: Vec<ModelInfo>,
    /// Nom du modele en cours de chargement (affiche un spinner).
    loading_model: Option<String>,
    stream_handle: Option<tokio::task::JoinHandle<()>>,
    reasoning_enabled: bool,
    /// Limite de tokens par reponse. 0 = Auto (calcule a partir de la longueur du prompt).
    max_tokens: u32,
    /// Predicteur V1 — apprend combien de tokens le modele utilise vraiment.
    predictor: Predictor,
    /// Infos sur la derniere requete en cours (pour alimenter le predicteur a la fin).
    pending_stats: Option<PendingStats>,
    /// Affiche ou non le panneau predicteur a droite.
    show_predictor: bool,
    /// Sycometer : % de flatterie par modele.
    syco: SycoMeter,
    /// Affiche ou non le panneau sycometer a gauche.
    show_syco: bool,
    /// Affiche ou non l'arborescence de fichiers a gauche (sous le sycometer).
    show_file_tree: bool,
    /// Racine affichee par l'arborescence. Vide = parent de l'executable.
    workspace_path: String,
    /// Dossier actif pour l'IA (clic droit sur un dossier dans l'arbo). Persiste.
    ai_workdir: String,
    /// Dossier en attente de confirmation (popup ouverte tant que Some).
    pending_workdir: Option<String>,
    /// Buffer de recherche dans l'arborescence (vide = mode arbre, non vide = mode liste plate).
    tree_search: String,
    /// Cache des listings de dossiers (TTL ~2s) pour eviter de scanner le disque a 60fps.
    tree_cache: HashMap<PathBuf, (Instant, Vec<TreeEntry>)>,
    /// Cache du dernier resultat de recherche (query, resultats).
    tree_search_cache: Option<(String, Vec<TreeEntry>, Instant)>,
    /// Vue active : Chat / Persona / Parametres.
    view: View,
    /// System prompt persiste dans system_prompt.txt. Envoye en role:"system" au debut de chaque requete si non vide.
    system_prompt: String,
    /// Etat du panneau System Prompt dans l'onglet Persona (plie/deplie).
    system_prompt_open: bool,
    /// Parametres de sampling envoyes dans la requete HTTP (persistes dans settings.json).
    temperature: f32,
    top_p: f32,
    frequency_penalty: f32,
    presence_penalty: f32,
    seed: Option<i64>,
    /// Vrai si le dernier message assistant a ete tronque (reasoning sans content).
    /// Sert a highlighter le ComboBox max_tokens dans la barre du haut.
    last_truncated: bool,
    /// Tools actives : le modele peut appeler list_dir, read_file, write_file, make_dir.
    tools_enabled: bool,
    /// Derniere source Mermaid generee (flow de pensee du modele).
    thought_flow: String,
    /// Affiche ou non le panneau thought flow.
    show_thought_flow: bool,
    rx: Receiver<Incoming>,
    tx: Sender<Incoming>,
    runtime: tokio::runtime::Runtime,
}

impl Default for App {
    fn default() -> Self {
        let (tx, rx) = channel();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime");
        // Liste les modeles au demarrage
        let tx_init = tx.clone();
        runtime.spawn(async move {
            let list = list_all_models().await;
            let _ = tx_init.send(Incoming::ModelsList(list));
        });
        let settings = load_settings();
        Self {
            input: String::new(),
            messages: Vec::new(),
            waiting: false,
            model: String::new(),
            available: Vec::new(),
            loading_model: None,
            stream_handle: None,
            reasoning_enabled: settings.reasoning_default,
            max_tokens: settings.max_tokens_default,
            predictor: Predictor::default(),
            pending_stats: None,
            show_predictor: settings.show_predictor,
            syco: SycoMeter::default(),
            show_syco: settings.show_syco,
            show_file_tree: settings.show_file_tree,
            workspace_path: settings.workspace_path,
            ai_workdir: settings.ai_workdir,
            pending_workdir: None,
            tree_search: String::new(),
            tree_cache: HashMap::new(),
            tree_search_cache: None,
            view: View::Chat,
            system_prompt: load_system_prompt(),
            system_prompt_open: true,
            temperature: settings.temperature,
            top_p: settings.top_p,
            frequency_penalty: settings.frequency_penalty,
            presence_penalty: settings.presence_penalty,
            seed: settings.seed,
            last_truncated: false,
            tools_enabled: settings.tools_enabled,
            thought_flow: String::new(),
            show_thought_flow: false,
            rx,
            tx,
            runtime,
        }
    }
}

impl App {
    fn persist_settings(&self) {
        save_settings(&Settings {
            show_predictor: self.show_predictor,
            show_syco: self.show_syco,
            show_file_tree: self.show_file_tree,
            workspace_path: self.workspace_path.clone(),
            ai_workdir: self.ai_workdir.clone(),
            reasoning_default: self.reasoning_enabled,
            max_tokens_default: self.max_tokens,
            temperature: self.temperature,
            top_p: self.top_p,
            frequency_penalty: self.frequency_penalty,
            presence_penalty: self.presence_penalty,
            seed: self.seed,
            tools_enabled: self.tools_enabled,
        });
    }

    /// Racine effective de l'arborescence (champ utilisateur si non vide, sinon parent de l'exe).
    fn workspace_root(&self) -> PathBuf {
        let trimmed = self.workspace_path.trim();
        if trimmed.is_empty() {
            return default_workspace();
        }
        // Windows : "h:" sans slash pointe sur le cwd du drive, pas sur sa racine.
        // On normalise "x:" -> "x:\" pour que l'utilisateur voie bien le haut du disque.
        if trimmed.len() == 2
            && trimmed.as_bytes()[1] == b':'
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
        {
            return PathBuf::from(format!("{}\\", trimmed));
        }
        PathBuf::from(trimmed)
    }

    /// Renvoie la liste des entrees d'un dossier, avec cache TTL 2s.
    fn tree_dir_entries(&mut self, path: &Path) -> Vec<TreeEntry> {
        let refresh = self
            .tree_cache
            .get(path)
            .map(|(t, _)| t.elapsed() > Duration::from_secs(2))
            .unwrap_or(true);
        if refresh {
            let entries = read_dir_limited(path, 500);
            self.tree_cache
                .insert(path.to_path_buf(), (Instant::now(), entries));
        }
        self.tree_cache
            .get(path)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
}

/// Entree de l'arborescence (fichier ou dossier). Path absolu.
#[derive(Clone)]
struct TreeEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    size: u64,
    truncated: bool, // true = "... (N autres)" placeholder
}

/// Liste les enfants directs d'un dossier, tries (dossiers d'abord, alpha).
/// Si le dossier contient plus de `limit` entrees, on tronque + marqueur.
fn read_dir_limited(path: &Path, limit: usize) -> Vec<TreeEntry> {
    let Ok(rd) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut out: Vec<TreeEntry> = Vec::new();
    let mut total = 0usize;
    for entry in rd.flatten() {
        total += 1;
        if out.len() >= limit {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let p = entry.path();
        let (is_dir, size) = match entry.metadata() {
            Ok(m) => (m.is_dir(), m.len()),
            Err(_) => (false, 0),
        };
        out.push(TreeEntry {
            name,
            path: p,
            is_dir,
            size,
            truncated: false,
        });
    }
    out.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    if total > limit {
        out.push(TreeEntry {
            name: format!("… ({} autres)", total - limit),
            path: path.to_path_buf(),
            is_dir: false,
            size: 0,
            truncated: true,
        });
    }
    out
}

/// Recherche recursive (BFS) dans le workspace. Limites strictes pour pas geler l'UI.
fn search_recursive(root: &Path, query: &str, max_depth: u32, max_results: usize) -> Vec<TreeEntry> {
    let q = query.to_lowercase();
    let mut out: Vec<TreeEntry> = Vec::new();
    let mut queue: std::collections::VecDeque<(PathBuf, u32)> = std::collections::VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));
    while let Some((dir, depth)) = queue.pop_front() {
        if out.len() >= max_results {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            if out.len() >= max_results {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let p = entry.path();
            let (is_dir, size) = match entry.metadata() {
                Ok(m) => (m.is_dir(), m.len()),
                Err(_) => (false, 0),
            };
            if name.to_lowercase().contains(&q) {
                out.push(TreeEntry {
                    name: name.clone(),
                    path: p.clone(),
                    is_dir,
                    size,
                    truncated: false,
                });
            }
            if is_dir && depth + 1 < max_depth {
                queue.push_back((p, depth + 1));
            }
        }
    }
    out
}

/// Format "1.2 KB" / "3.4 MB" / "12 B".
fn format_size(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n >= GB {
        format!("{:.1} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{} B", n)
    }
}

async fn list_all_models() -> Vec<ModelInfo> {
    let client = reqwest::Client::new();
    let Ok(resp) = client
        .get("http://localhost:1234/api/v1/models")
        .send()
        .await
    else {
        return Vec::new();
    };
    let Ok(json) = resp.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    let Some(arr) = json["models"].as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter(|m| m["type"].as_str() == Some("llm"))
        .filter_map(|m| {
            let id = m["key"].as_str()?.to_string();
            let loaded = m["loaded_instances"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            Some(ModelInfo { id, loaded })
        })
        .collect()
}

async fn load_model(id: String) -> Result<String, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({ "model": id.clone() });
    let resp = client
        .post("http://localhost:1234/api/v1/models/load")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LM Studio {} : {}", status, body));
    }
    Ok(id)
}

// ============================================================
// Knowledge DB — base vectorielle SQLite + nomic-embed
// knowledge.db a cote de l'exe. Embeddings 768 dims stockes en BLOB.
// Cosine similarity maison, pas de crate vectoriel externe.
// ============================================================

const EMBED_URL: &str = "http://localhost:1234/v1/embeddings";
const EMBED_MODEL: &str = "text-embedding-nomic-embed-text-v1.5";

fn knowledge_db_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("knowledge.db")))
        .unwrap_or_else(|| PathBuf::from("knowledge.db"))
}

fn open_knowledge_db() -> Result<rusqlite::Connection, String> {
    let path = knowledge_db_path();
    let conn = rusqlite::Connection::open(&path)
        .map_err(|e| format!("DB open: {}", e))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS knowledge (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            tags TEXT,
            embedding BLOB NOT NULL,
            source TEXT,
            created_at TEXT,
            model TEXT
        )",
        [],
    )
    .map_err(|e| format!("DB init: {}", e))?;
    Ok(conn)
}

async fn embed_text(client: &reqwest::Client, text: &str) -> Result<Vec<f32>, String> {
    let body = serde_json::json!({
        "model": EMBED_MODEL,
        "input": text,
    });
    let resp = client
        .post(EMBED_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("embed HTTP: {}", e))?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(format!("embed {}: {}", s, b));
    }
    let j: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("embed JSON: {}", e))?;
    let arr = j["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| "reponse sans embedding".to_string())?;
    let vec: Vec<f32> = arr
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();
    if vec.is_empty() {
        return Err("embedding vide".into());
    }
    Ok(vec)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut ma = 0.0f32;
    let mut mb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        ma += a[i] * a[i];
        mb += b[i] * b[i];
    }
    if ma == 0.0 || mb == 0.0 {
        return 0.0;
    }
    dot / (ma.sqrt() * mb.sqrt())
}

fn floats_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

fn blob_to_floats(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

// ============================================================
// Task State DB — base ephemere pour les cycles d'agent
// task_state.db a cote de l'exe. 3 tables : tasks, steps, cycle_prompts.
// Complementaire a knowledge.db (durable, curee).
// ============================================================

fn task_db_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("task_state.db")))
        .unwrap_or_else(|| PathBuf::from("task_state.db"))
}

fn open_task_db() -> Result<rusqlite::Connection, String> {
    let path = task_db_path();
    let conn = rusqlite::Connection::open(&path)
        .map_err(|e| format!("task DB open: {}", e))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_description TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            ended_at TEXT,
            model TEXT,
            final_summary TEXT
         );
         CREATE TABLE IF NOT EXISTS steps (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL REFERENCES tasks(id),
            step_number INTEGER NOT NULL,
            description TEXT NOT NULL,
            status TEXT NOT NULL,
            findings TEXT,
            error TEXT,
            tool_calls_json TEXT,
            started_at TEXT,
            ended_at TEXT,
            tokens_used INTEGER
         );
         CREATE TABLE IF NOT EXISTS cycle_prompts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL REFERENCES tasks(id),
            cycle_number INTEGER NOT NULL,
            system_prompt TEXT,
            user_prompt TEXT,
            response_text TEXT,
            ts TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_steps_task ON steps(task_id);
         CREATE INDEX IF NOT EXISTS idx_cycle_prompts_task ON cycle_prompts(task_id);",
    )
    .map_err(|e| format!("task DB init: {}", e))?;
    Ok(conn)
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

/// Decodage robuste d'un stdout Windows : UTF-8 strict, fallback Windows-1252.
/// Utile car PowerShell/cmd sortent souvent en CP1252 (accents francais) pas en UTF-8.
fn decode_output(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    bytes.iter().map(|&b| cp1252_decode(b)).collect()
}

fn cp1252_decode(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}', 0x82 => '\u{201A}', 0x83 => '\u{0192}', 0x84 => '\u{201E}',
        0x85 => '\u{2026}', 0x86 => '\u{2020}', 0x87 => '\u{2021}', 0x88 => '\u{02C6}',
        0x89 => '\u{2030}', 0x8A => '\u{0160}', 0x8B => '\u{2039}', 0x8C => '\u{0152}',
        0x8E => '\u{017D}', 0x91 => '\u{2018}', 0x92 => '\u{2019}', 0x93 => '\u{201C}',
        0x94 => '\u{201D}', 0x95 => '\u{2022}', 0x96 => '\u{2013}', 0x97 => '\u{2014}',
        0x98 => '\u{02DC}', 0x99 => '\u{2122}', 0x9A => '\u{0161}', 0x9B => '\u{203A}',
        0x9C => '\u{0153}', 0x9E => '\u{017E}', 0x9F => '\u{0178}',
        _ => char::from_u32(b as u32).unwrap_or('?'),
    }
}

// ============================================================
// Tools — definitions, acces, execution
// ============================================================

fn tool_definitions() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "List files and directories at the given path. Returns name, type (file/dir), and size for each entry. Capped at 200 entries.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path to list (relative to workdir)" }
                    },
                    "required": ["path"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read the content of a text file. Returns the text content. Capped at 1MB.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to read (relative to workdir)" },
                        "start_line": { "type": "integer", "description": "Optional: first line to read (1-based)" },
                        "end_line": { "type": "integer", "description": "Optional: last line to read (1-based, inclusive)" }
                    },
                    "required": ["path"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write content to a file. Creates parent directories if needed. Overwrites existing content.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to write (relative to workdir)" },
                        "content": { "type": "string", "description": "Content to write" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "make_dir",
                "description": "Create a directory (and parent directories if needed).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path to create (relative to workdir)" }
                    },
                    "required": ["path"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Edit a file by replacing an exact string with a new string. The old_string must appear exactly once in the file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to edit (relative to workdir)" },
                        "old_string": { "type": "string", "description": "Exact string to find and replace (must be unique in the file)" },
                        "new_string": { "type": "string", "description": "Replacement string" }
                    },
                    "required": ["path", "old_string", "new_string"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Execute a shell command in the workdir. Returns stdout and stderr. Timeout 30 seconds. Use cmd /C on Windows.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Shell command to execute" }
                    },
                    "required": ["command"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "save_knowledge",
                "description": "Save a piece of knowledge to the persistent vector database. The content will be embedded (nomic-embed-text) and stored for semantic search. Use this to remember facts, user preferences, decisions, insights — anything worth retrieving later.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Short title (one line)" },
                        "content": { "type": "string", "description": "The full content to remember" },
                        "tags": { "type": "string", "description": "Optional comma-separated tags (e.g. 'user,preference')" }
                    },
                    "required": ["title", "content"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "search_knowledge",
                "description": "Semantic search over the persistent knowledge database. Returns the top N most relevant entries with cosine similarity score. Use this before answering questions that might have been addressed before.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What to search for (natural language)" },
                        "limit": { "type": "integer", "description": "Max results to return (default 5)" }
                    },
                    "required": ["query"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "list_knowledge",
                "description": "List stored knowledge entries (id, title, tags). Optionally filter by a tag substring.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tag": { "type": "string", "description": "Optional tag substring filter" }
                    }
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "delete_knowledge",
                "description": "Delete a knowledge entry by its id.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer", "description": "Entry id to delete" }
                    },
                    "required": ["id"]
                }
            }
        }),
    ]
}

/// Resout un chemin relatif ou absolu par rapport au workdir,
/// puis verifie que le resultat est DANS le workdir (jail).
fn check_access(raw: &str, workdir: &Path) -> Result<PathBuf, String> {
    if workdir.as_os_str().is_empty() {
        return Err("Aucun workdir IA defini. Definissez-en un (clic droit dans l'arborescence).".into());
    }
    let candidate = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        workdir.join(raw)
    };
    // Canonicalize le workdir (doit exister)
    let canon_wd = workdir.canonicalize().map_err(|e| format!("workdir invalide: {}", e))?;
    // Pour le candidat, on canonicalize le parent (le fichier peut ne pas encore exister)
    let canon = if candidate.exists() {
        candidate.canonicalize().map_err(|e| format!("chemin invalide: {}", e))?
    } else {
        let parent = candidate.parent().ok_or("chemin invalide: pas de parent")?;
        let canon_parent = parent.canonicalize().map_err(|e| format!("parent invalide: {}", e))?;
        let name = candidate.file_name().ok_or("chemin invalide: pas de nom de fichier")?;
        canon_parent.join(name)
    };
    if !canon.starts_with(&canon_wd) {
        return Err(format!(
            "ACCES REFUSE : {} est hors du workdir {}",
            canon.display(),
            canon_wd.display()
        ));
    }
    Ok(canon)
}

/// Execute un tool et retourne (resultat_texte, is_error).
async fn execute_tool(
    name: &str,
    args: &serde_json::Value,
    workdir: &Path,
    client: &reqwest::Client,
    model: &str,
) -> (String, bool) {
    match name {
        "list_dir" => {
            let raw = args["path"].as_str().unwrap_or(".");
            let path = match check_access(raw, workdir) {
                Ok(p) => p,
                Err(e) => return (e, true),
            };
            let entries = read_dir_limited(&path, 200);
            if entries.is_empty() {
                return ("(vide ou inaccessible)".into(), false);
            }
            let mut out = String::new();
            for e in &entries {
                if e.name.starts_with("_thought_flow.") {
                    continue;
                }
                if e.truncated {
                    out.push_str(&format!("{}\n", e.name));
                } else if e.is_dir {
                    out.push_str(&format!("[dir]  {}\n", e.name));
                } else {
                    out.push_str(&format!("[file] {} ({})\n", e.name, format_size(e.size)));
                }
            }
            if out.is_empty() {
                return ("(vide ou inaccessible)".into(), false);
            }
            (out, false)
        }
        "read_file" => {
            let raw = args["path"].as_str().unwrap_or("");
            let path = match check_access(raw, workdir) {
                Ok(p) => p,
                Err(e) => return (e, true),
            };
            // Cap 1 Mo
            match std::fs::metadata(&path) {
                Ok(m) if m.len() > 1_048_576 => {
                    return (format!("Fichier trop gros ({}) — max 1 Mo", format_size(m.len())), true);
                }
                Err(e) => return (format!("Erreur : {}", e), true),
                _ => {}
            }
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let start = args["start_line"].as_u64().map(|n| n.max(1) as usize);
                    let end = args["end_line"].as_u64().map(|n| n as usize);
                    if let Some(s) = start {
                        let lines: Vec<&str> = content.lines().collect();
                        let e = end.unwrap_or(lines.len()).min(lines.len());
                        let s = (s - 1).min(lines.len());
                        let slice = &lines[s..e];
                        let mut out = String::new();
                        for (i, line) in slice.iter().enumerate() {
                            out.push_str(&format!("{:4} | {}\n", s + i + 1, line));
                        }
                        (out, false)
                    } else {
                        (content, false)
                    }
                }
                Err(e) => (format!("Erreur lecture : {}", e), true),
            }
        }
        "write_file" => {
            let raw = args["path"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");
            let path = match check_access(raw, workdir) {
                Ok(p) => p,
                Err(e) => return (e, true),
            };
            // Cree les parents si necessaire
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&path, content) {
                Ok(()) => (format!("OK — {} octets ecrits dans {}", content.len(), path.display()), false),
                Err(e) => (format!("Erreur ecriture : {}", e), true),
            }
        }
        "make_dir" => {
            let raw = args["path"].as_str().unwrap_or("");
            let path = match check_access(raw, workdir) {
                Ok(p) => p,
                Err(e) => return (e, true),
            };
            match std::fs::create_dir_all(&path) {
                Ok(()) => (format!("OK — dossier cree : {}", path.display()), false),
                Err(e) => (format!("Erreur creation dossier : {}", e), true),
            }
        }
        "edit_file" => {
            let raw = args["path"].as_str().unwrap_or("");
            let old = args["old_string"].as_str().unwrap_or("");
            let new = args["new_string"].as_str().unwrap_or("");
            if old.is_empty() {
                return ("old_string ne peut pas etre vide".into(), true);
            }
            let path = match check_access(raw, workdir) {
                Ok(p) => p,
                Err(e) => return (e, true),
            };
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => return (format!("Erreur lecture : {}", e), true),
            };
            let count = content.matches(old).count();
            if count == 0 {
                return (format!("old_string introuvable dans {}", path.display()), true);
            }
            if count > 1 {
                return (format!("old_string trouvee {} fois (doit etre unique) dans {}", count, path.display()), true);
            }
            let new_content = content.replacen(old, new, 1);
            match std::fs::write(&path, &new_content) {
                Ok(()) => (format!("OK — edit applique dans {} ({} chars remplaces)", path.display(), old.len()), false),
                Err(e) => (format!("Erreur ecriture : {}", e), true),
            }
        }
        "run_command" => {
            let cmd = args["command"].as_str().unwrap_or("");
            if cmd.is_empty() {
                return ("command ne peut pas etre vide".into(), true);
            }
            // chcp 65001 force cmd.exe en UTF-8 avant d'invoquer la commande.
            // Le sous-processus herite en general du codepage. Le decodage stdout fait
            // ensuite un fallback CP1252 si l'UTF-8 n'est pas strict (voir decode_output).
            let wrapped = format!("chcp 65001 >nul & {}", cmd);
            // Execute via cmd /C sur Windows, timeout 30s, kill si depasse
            match std::process::Command::new("cmd")
                .args(["/C", &wrapped])
                .current_dir(workdir)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => {
                    let pid = child.id();
                    let (done_tx, done_rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = done_tx.send(child.wait_with_output());
                    });
                    match done_rx.recv_timeout(Duration::from_secs(30)) {
                        Ok(Ok(output)) => {
                            let stdout = decode_output(&output.stdout);
                            let stderr = decode_output(&output.stderr);
                            let exit = output.status.code().unwrap_or(-1);
                            let mut out = String::new();
                            if !stdout.is_empty() {
                                let s: String = stdout.chars().take(10_000).collect();
                                out.push_str(&s);
                                if stdout.len() > 10_000 {
                                    out.push_str("\n... (tronque a 10KB)");
                                }
                            }
                            if !stderr.is_empty() {
                                if !out.is_empty() { out.push('\n'); }
                                out.push_str("[stderr] ");
                                let s: String = stderr.chars().take(5_000).collect();
                                out.push_str(&s);
                            }
                            if out.is_empty() {
                                out = format!("(pas de sortie, exit code {})", exit);
                            } else {
                                out.push_str(&format!("\n[exit {}]", exit));
                            }
                            (out, exit != 0)
                        }
                        Ok(Err(e)) => (format!("Erreur : {}", e), true),
                        Err(_) => {
                            // Timeout — kill le process et ses enfants via taskkill
                            let _ = std::process::Command::new("taskkill")
                                .args(["/F", "/PID", &pid.to_string(), "/T"])
                                .output();
                            ("TIMEOUT (30s) — commande interrompue. Le process a ete tue.".into(), true)
                        }
                    }
                }
                Err(e) => (format!("Erreur execution : {}", e), true),
            }
        }
        "save_knowledge" => {
            let title = args["title"].as_str().unwrap_or("").to_string();
            let content = args["content"].as_str().unwrap_or("").to_string();
            let tags = args["tags"].as_str().unwrap_or("").to_string();
            if content.trim().is_empty() {
                return ("content vide".into(), true);
            }
            let emb = match embed_text(client, &content).await {
                Ok(v) => v,
                Err(e) => return (format!("embedding: {}", e), true),
            };
            let conn = match open_knowledge_db() {
                Ok(c) => c,
                Err(e) => return (e, true),
            };
            let blob = floats_to_blob(&emb);
            let now = current_timestamp();
            match conn.execute(
                "INSERT INTO knowledge (title, content, tags, embedding, source, created_at, model)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![title, content, tags, blob, "model", now, model],
            ) {
                Ok(_) => {
                    let id = conn.last_insert_rowid();
                    (
                        format!("OK — #{} sauvegarde ({} dims, {} octets)", id, emb.len(), content.len()),
                        false,
                    )
                }
                Err(e) => (format!("DB insert: {}", e), true),
            }
        }
        "search_knowledge" => {
            let query = args["query"].as_str().unwrap_or("").to_string();
            let limit = args["limit"].as_u64().unwrap_or(5).max(1).min(50) as usize;
            if query.trim().is_empty() {
                return ("query vide".into(), true);
            }
            let q_emb = match embed_text(client, &query).await {
                Ok(v) => v,
                Err(e) => return (format!("embedding: {}", e), true),
            };
            let conn = match open_knowledge_db() {
                Ok(c) => c,
                Err(e) => return (e, true),
            };
            let mut stmt = match conn
                .prepare("SELECT id, title, content, tags, embedding FROM knowledge")
            {
                Ok(s) => s,
                Err(e) => return (format!("DB prepare: {}", e), true),
            };
            let iter = match stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    row.get::<_, Vec<u8>>(4)?,
                ))
            }) {
                Ok(it) => it,
                Err(e) => return (format!("DB query: {}", e), true),
            };
            let mut scored: Vec<(i64, String, String, String, f32)> = Vec::new();
            for row in iter.flatten() {
                let emb = blob_to_floats(&row.4);
                let score = cosine_similarity(&q_emb, &emb);
                scored.push((row.0, row.1, row.2, row.3, score));
            }
            if scored.is_empty() {
                return ("(base vide — aucune connaissance sauvegardee)".into(), false);
            }
            scored.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(limit);
            let mut out = String::new();
            for (id, title, content, tags, score) in scored {
                let snippet: String = content.chars().take(400).collect();
                let more = if content.chars().count() > 400 { "..." } else { "" };
                out.push_str(&format!(
                    "#{} [score {:.3}] {}\n  tags: {}\n  {}{}\n\n",
                    id,
                    score,
                    title,
                    if tags.is_empty() { "(aucun)" } else { &tags },
                    snippet,
                    more
                ));
            }
            (out, false)
        }
        "list_knowledge" => {
            let tag = args["tag"].as_str().unwrap_or("").trim().to_string();
            let conn = match open_knowledge_db() {
                Ok(c) => c,
                Err(e) => return (e, true),
            };
            let rows: Vec<(i64, String, String, String)> = if !tag.is_empty() {
                let like = format!("%{}%", tag);
                let mut stmt = match conn.prepare(
                    "SELECT id, title, tags, created_at FROM knowledge WHERE tags LIKE ?1 ORDER BY id DESC",
                ) {
                    Ok(s) => s,
                    Err(e) => return (format!("DB prepare: {}", e), true),
                };
                let iter = match stmt.query_map([like], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    ))
                }) {
                    Ok(it) => it,
                    Err(e) => return (format!("DB query: {}", e), true),
                };
                iter.flatten().collect()
            } else {
                let mut stmt = match conn.prepare(
                    "SELECT id, title, tags, created_at FROM knowledge ORDER BY id DESC",
                ) {
                    Ok(s) => s,
                    Err(e) => return (format!("DB prepare: {}", e), true),
                };
                let iter = match stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    ))
                }) {
                    Ok(it) => it,
                    Err(e) => return (format!("DB query: {}", e), true),
                };
                iter.flatten().collect()
            };
            if rows.is_empty() {
                return ("(aucune entree)".into(), false);
            }
            let mut out = format!("{} entree(s) :\n", rows.len());
            for (id, title, tags, _ts) in rows {
                out.push_str(&format!(
                    "#{} — {} [{}]\n",
                    id,
                    title,
                    if tags.is_empty() { "aucun tag".to_string() } else { tags }
                ));
            }
            (out, false)
        }
        "delete_knowledge" => {
            let id = match args["id"].as_i64() {
                Some(n) => n,
                None => return ("id manquant ou invalide".into(), true),
            };
            let conn = match open_knowledge_db() {
                Ok(c) => c,
                Err(e) => return (e, true),
            };
            match conn.execute("DELETE FROM knowledge WHERE id = ?1", rusqlite::params![id]) {
                Ok(0) => (format!("#{} introuvable", id), true),
                Ok(n) => (format!("OK — {} entree(s) supprimee(s) (#{}) ", n, id), false),
                Err(e) => (format!("DB delete: {}", e), true),
            }
        }
        _ => (format!("Tool inconnu : {}", name), true),
    }
}

/// Echappe un texte pour Mermaid (retire les caracteres qui cassent le parser).
fn mermaid_escape(s: &str, max_chars: usize) -> String {
    let short: String = s.chars().take(max_chars).collect();
    short
        .replace('"', "'")
        .replace('\n', " ")
        .replace('(', " ")
        .replace(')', " ")
        .replace('[', " ")
        .replace(']', " ")
        .replace('{', " ")
        .replace('}', " ")
        .replace('|', "/")
        .replace('<', " ")
        .replace('>', " ")
        .replace('#', " ")
        .replace('&', "+")
        .replace(';', ",")
}

/// Genere un diagramme Mermaid a partir des tool calls du dernier message.
fn generate_thought_flow(msg: &Msg, user_prompt: &str) -> String {
    let mut mermaid = String::from("flowchart TD\n");
    // Noeud user
    let user_short = mermaid_escape(user_prompt, 50);
    mermaid.push_str(&format!("    U[\"User: {}\"]\n", user_short));
    if msg.tool_calls.is_empty() {
        mermaid.push_str("    U --> R\n");
    } else {
        mermaid.push_str("    U --> T0\n");
    }
    for (i, tc) in msg.tool_calls.iter().enumerate() {
        let args_short = mermaid_escape(&tc.arguments, 35);
        let result_short = mermaid_escape(&tc.result, 35);
        let status = if tc.is_error { "ERREUR" } else { "OK" };
        let node_id = format!("T{}", i);
        let next_id = if i + 1 < msg.tool_calls.len() {
            format!("T{}", i + 1)
        } else {
            "R".to_string()
        };
        // Noeud tool
        mermaid.push_str(&format!(
            "    {}[\"{}  {}\"]\n",
            node_id, tc.name, args_short
        ));
        // Fleche avec resultat
        mermaid.push_str(&format!(
            "    {} -->|{}: {}| {}\n",
            node_id, status, result_short, next_id
        ));
    }
    // Noeud reponse
    let resp_short = mermaid_escape(&msg.content, 50);
    mermaid.push_str(&format!("    R[\"Reponse: {}\"]\n", resp_short));
    // Styles
    mermaid.push_str("    style U fill:#2a4a7f,stroke:#5588cc,color:#fff\n");
    mermaid.push_str("    style R fill:#2a6f2a,stroke:#55cc55,color:#fff\n");
    for (i, tc) in msg.tool_calls.iter().enumerate() {
        let color = if tc.is_error { "#7f2a2a" } else { "#5a4a1a" };
        let stroke = if tc.is_error { "#cc5555" } else { "#ccaa44" };
        mermaid.push_str(&format!(
            "    style T{} fill:{},stroke:{},color:#fff\n", i, color, stroke
        ));
    }
    mermaid
}

/// Sauve le flow de pensee en .md (lisible par le modele) et .html (rendu graphique).
fn save_thought_flow(mermaid: &str, workdir: &str) {
    if workdir.trim().is_empty() {
        return;
    }
    let wd = Path::new(workdir);
    // Sauve le source Mermaid (.md)
    let md_path = wd.join("_thought_flow.md");
    let md_content = format!("# Thought Flow\n\n```mermaid\n{}\n```\n", mermaid);
    let _ = std::fs::write(&md_path, &md_content);
    // Sauve le HTML rendu
    let html_path = wd.join("_thought_flow.html");
    let html = format!(
        r#"<!DOCTYPE html>
<html><head>
<meta charset="UTF-8">
<title>Thought Flow</title>
<style>
body {{ background: #1a1a2e; display: flex; justify-content: center; padding: 40px; }}
.mermaid {{ background: #16213e; padding: 30px; border-radius: 12px; }}
</style>
</head><body>
<pre class="mermaid">
{}
</pre>
<script src="https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.min.js"></script>
<script>mermaid.initialize({{ startOnLoad: true, theme: 'dark' }});</script>
</body></html>"#,
        mermaid
    );
    let _ = std::fs::write(&html_path, &html);
}

async fn stream_to_lm_studio(
    model: String,
    history: Vec<(Role, String)>,
    reasoning_enabled: bool,
    max_tokens: u32,
    system_prompt: String,
    sampling: SamplingParams,
    tools_enabled: bool,
    ai_workdir: String,
    tx: Sender<Incoming>,
) {
    use futures_util::StreamExt;

    // Construit les messages comme Vec<Value> pour supporter la boucle tools.
    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(history.len() + 2);

    // System prompt + injection auto du contexte workdir si tools actives
    let mut sys = system_prompt.clone();
    if tools_enabled && !ai_workdir.trim().is_empty() {
        let wd = PathBuf::from(&ai_workdir);
        if wd.is_dir() {
            let mut ctx_parts: Vec<String> = Vec::new();
            ctx_parts.push(format!("Workdir: {}", wd.display()));
            // Detection du type de projet
            if wd.join("Cargo.toml").exists() { ctx_parts.push("Projet Rust (Cargo.toml)".into()); }
            if wd.join("package.json").exists() { ctx_parts.push("Projet Node (package.json)".into()); }
            if wd.join("requirements.txt").exists() { ctx_parts.push("Projet Python (requirements.txt)".into()); }
            if wd.join("pyproject.toml").exists() { ctx_parts.push("Projet Python (pyproject.toml)".into()); }
            if wd.join("venv").is_dir() || wd.join(".venv").is_dir() { ctx_parts.push("venv detecte".into()); }
            // Liste rapide du contenu (top 15), sans les artefacts _thought_flow.*
            if let Ok(rd) = std::fs::read_dir(&wd) {
                let items: Vec<String> = rd.flatten()
                    .filter(|e| !e.file_name().to_string_lossy().starts_with("_thought_flow."))
                    .take(15)
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        let is_dir = e.metadata().map(|m| m.is_dir()).unwrap_or(false);
                        if is_dir { format!("{}/", name) } else { name }
                    })
                    .collect();
                if !items.is_empty() {
                    ctx_parts.push(format!("Contenu: {}", items.join(", ")));
                }
            }
            let ctx_line = format!("\n\n[Contexte workdir IA]\n{}", ctx_parts.join("\n"));
            sys.push_str(&ctx_line);
        }
    }
    if !sys.trim().is_empty() {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }
    for (r, c) in &history {
        let role = match r {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        messages.push(serde_json::json!({"role": role, "content": c}));
    }

    let tool_defs = if tools_enabled && !ai_workdir.trim().is_empty() {
        Some(tool_definitions())
    } else {
        None
    };
    let workdir = PathBuf::from(&ai_workdir);

    let client = reqwest::Client::new();

    // Boucle tool_use : max 10 iterations.
    for iteration in 0u32..10 {
        // Construit le body de la requete
        let mut req = serde_json::json!({
            "model": model,
            "messages": messages,
            "temperature": sampling.temperature,
            "top_p": sampling.top_p,
            "frequency_penalty": sampling.frequency_penalty,
            "presence_penalty": sampling.presence_penalty,
            "max_tokens": max_tokens,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if let Some(seed) = sampling.seed {
            req["seed"] = serde_json::json!(seed);
        }
        if !reasoning_enabled {
            req["reasoning"] = serde_json::json!("off");
            req["reasoning_effort"] = serde_json::json!("minimal");
            req["chat_template_kwargs"] = serde_json::json!({"enable_thinking": false});
        }
        if let Some(ref tools) = tool_defs {
            req["tools"] = serde_json::json!(tools);
            req["tool_choice"] = serde_json::json!("auto");
        }

        if iteration > 0 {
            let _ = tx.send(Incoming::ToolLoopIteration(iteration));
        }

        let resp = match client.post(LM_STUDIO_URL).json(&req).send().await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(Incoming::StreamError(format!("HTTP error: {}", e)));
                return;
            }
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let _ = tx.send(Incoming::StreamError(format!("LM Studio {} : {}", status, body)));
            return;
        }

        // Buffer pour les tool_calls streames (index -> (id, name, args_buffer))
        let mut tc_buffer: Vec<(String, String, String)> = Vec::new();
        let mut got_tool_calls = false;
        let mut content_so_far = String::new();

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut stream_done = false;
        while let Some(chunk_res) = stream.next().await {
            let chunk = match chunk_res {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(Incoming::StreamError(format!("Stream error: {}", e)));
                    return;
                }
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buffer.find("\n\n") {
                let event = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();
                for line in event.lines() {
                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };
                    if data.trim() == "[DONE]" {
                        stream_done = true;
                        break;
                    }
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        let delta = &json["choices"][0]["delta"];
                        // Reasoning tokens
                        if let Some(r) = delta["reasoning_content"].as_str() {
                            if !r.is_empty() {
                                let _ = tx.send(Incoming::ReasoningToken(r.to_string()));
                            }
                        }
                        // Content tokens
                        if let Some(c) = delta["content"].as_str() {
                            if !c.is_empty() {
                                content_so_far.push_str(c);
                                let _ = tx.send(Incoming::Token(c.to_string()));
                            }
                        }
                        // Tool calls (streames par morceaux)
                        if let Some(tcs) = delta["tool_calls"].as_array() {
                            got_tool_calls = true;
                            for tc in tcs {
                                let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                                // Agrandir le buffer si necessaire
                                while tc_buffer.len() <= idx {
                                    tc_buffer.push((String::new(), String::new(), String::new()));
                                }
                                if let Some(id) = tc["id"].as_str() {
                                    tc_buffer[idx].0 = id.to_string();
                                }
                                if let Some(name) = tc["function"]["name"].as_str() {
                                    tc_buffer[idx].1 = name.to_string();
                                }
                                if let Some(args) = tc["function"]["arguments"].as_str() {
                                    tc_buffer[idx].2.push_str(args);
                                }
                            }
                        }
                        // Usage stats (dernier chunk)
                        if let Some(used) = json["usage"]["completion_tokens"].as_u64() {
                            let finish = json["choices"][0]["finish_reason"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            let _ = tx.send(Incoming::Usage {
                                used: used as u32,
                                finish,
                            });
                        }
                    }
                }
                if stream_done {
                    break;
                }
            }
            if stream_done {
                break;
            }
        }

        // Si des tool_calls ont ete collectes, on les execute et on reboucle.
        if got_tool_calls && !tc_buffer.is_empty() {
            // Construire le message assistant avec tool_calls pour l'historique
            let mut tc_json: Vec<serde_json::Value> = Vec::new();
            for (id, name, args) in &tc_buffer {
                tc_json.push(serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": name, "arguments": args }
                }));
            }
            let assistant_msg = if content_so_far.is_empty() {
                serde_json::json!({
                    "role": "assistant",
                    "content": serde_json::Value::Null,
                    "tool_calls": tc_json
                })
            } else {
                serde_json::json!({
                    "role": "assistant",
                    "content": content_so_far,
                    "tool_calls": tc_json
                })
            };
            messages.push(assistant_msg);

            // Executer chaque tool et ajouter le resultat
            for (id, name, args_str) in &tc_buffer {
                let args: serde_json::Value = serde_json::from_str(args_str)
                    .unwrap_or(serde_json::json!({}));
                let (result, is_error) = execute_tool(name, &args, &workdir, &client, &model).await;
                // Envoyer a l'UI pour affichage
                let _ = tx.send(Incoming::ToolCallComplete(ToolCallInfo {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: args_str.clone(),
                    result: result.clone(),
                    is_error,
                }));
                // Ajouter le resultat dans l'historique pour le modele
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": result
                }));
            }
            // Reboucler : le modele va recevoir les resultats et repondre
            continue;
        }

        // Pas de tool_calls → fin normale
        let _ = tx.send(Incoming::StreamDone);
        return;
    }

    // Si on arrive ici, on a atteint le max d'iterations
    let _ = tx.send(Incoming::StreamError(
        "Boucle tool_use : max 10 iterations atteint".into(),
    ));
}

impl App {
    fn send_message(&mut self) {
        if self.input.trim().is_empty() || self.waiting || self.model.is_empty() {
            return;
        }
        let user_text = std::mem::take(&mut self.input);
        self.messages.push(Msg {
            role: Role::User,
            content: user_text,
            reasoning: String::new(),
            model: None,
            tool_calls: Vec::new(),
        });
        self.messages.push(Msg {
            role: Role::Assistant,
            content: String::new(),
            reasoning: String::new(),
            model: Some(self.model.clone()),
            tool_calls: Vec::new(),
        });
        self.waiting = true;
        let history: Vec<(Role, String)> = self
            .messages
            .iter()
            .filter(|m| !m.content.is_empty())
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        let model = self.model.clone();
        let reasoning = self.reasoning_enabled;
        // Longueur du dernier prompt user (pour bucket + predicteur)
        let last_user_chars = self
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.chars().count())
            .unwrap_or(0);
        // max_tokens = 0 → predicteur decide, sinon valeur fixe
        let max_tokens = if self.max_tokens == 0 {
            self.predictor.predict(&model, last_user_chars)
        } else {
            self.max_tokens
        };
        self.pending_stats = Some(PendingStats {
            model: model.clone(),
            prompt_chars: last_user_chars,
            allocated: max_tokens,
        });
        let tx = self.tx.clone();
        let system_prompt = self.system_prompt.clone();
        let sampling = SamplingParams {
            temperature: self.temperature,
            top_p: self.top_p,
            frequency_penalty: self.frequency_penalty,
            presence_penalty: self.presence_penalty,
            seed: self.seed,
        };
        let tools_on = self.tools_enabled;
        let workdir = self.ai_workdir.clone();
        let handle = self.runtime.spawn(async move {
            stream_to_lm_studio(model, history, reasoning, max_tokens, system_prompt, sampling, tools_on, workdir, tx).await;
        });
        self.stream_handle = Some(handle);
    }

    /// Relance la derniere requete assistant : retire le message assistant
    /// (potentiellement foireux) et renvoie le meme user au modele avec les
    /// parametres courants (max_tokens, reasoning, system_prompt).
    fn retry_last_assistant(&mut self) {
        if self.waiting || self.model.is_empty() || self.messages.is_empty() {
            return;
        }
        // Retire le dernier message s'il est assistant.
        if self
            .messages
            .last()
            .map(|m| m.role == Role::Assistant)
            .unwrap_or(false)
        {
            self.messages.pop();
        }
        // Il doit rester au moins un message user a la fin.
        if self
            .messages
            .last()
            .map(|m| m.role != Role::User)
            .unwrap_or(true)
        {
            return;
        }
        // Re-declenche la meme logique que send_message mais sans re-injecter de user.
        self.messages.push(Msg {
            role: Role::Assistant,
            content: String::new(),
            reasoning: String::new(),
            model: Some(self.model.clone()),
            tool_calls: Vec::new(),
        });
        self.waiting = true;
        let history: Vec<(Role, String)> = self
            .messages
            .iter()
            .filter(|m| !m.content.is_empty())
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        let model = self.model.clone();
        let reasoning = self.reasoning_enabled;
        let last_user_chars = self
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.chars().count())
            .unwrap_or(0);
        let max_tokens = if self.max_tokens == 0 {
            self.predictor.predict(&model, last_user_chars)
        } else {
            self.max_tokens
        };
        self.pending_stats = Some(PendingStats {
            model: model.clone(),
            prompt_chars: last_user_chars,
            allocated: max_tokens,
        });
        let tx = self.tx.clone();
        let system_prompt = self.system_prompt.clone();
        let sampling = SamplingParams {
            temperature: self.temperature,
            top_p: self.top_p,
            frequency_penalty: self.frequency_penalty,
            presence_penalty: self.presence_penalty,
            seed: self.seed,
        };
        let tools_on = self.tools_enabled;
        let workdir = self.ai_workdir.clone();
        let handle = self.runtime.spawn(async move {
            stream_to_lm_studio(model, history, reasoning, max_tokens, system_prompt, sampling, tools_on, workdir, tx).await;
        });
        self.stream_handle = Some(handle);
    }

    fn stop_generation(&mut self) {
        if let Some(h) = self.stream_handle.take() {
            h.abort();
        }
        self.waiting = false;
        if let Some(last) = self.messages.last_mut() {
            if last.role == Role::Assistant && last.content.is_empty() && last.reasoning.is_empty()
            {
                last.content = "[interrompu]".to_string();
            } else if last.role == Role::Assistant {
                last.content.push_str("\n[...interrompu]");
            }
        }
    }

    fn clear_conversation(&mut self) {
        self.stop_generation();
        self.messages.clear();
        self.pending_stats = None;
        self.last_truncated = false;
    }

    fn request_load(&mut self, model_id: String) {
        if self.loading_model.is_some() {
            return;
        }
        self.loading_model = Some(model_id.clone());
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = load_model(model_id).await;
            let _ = tx.send(Incoming::ModelLoaded(result));
        });
    }

    fn refresh_models(&mut self) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let list = list_all_models().await;
            let _ = tx.send(Incoming::ModelsList(list));
        });
    }

    fn drain_incoming(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Incoming::ModelsList(list) => {
                    self.available = list;
                    // Auto-selection : si un modele est deja loaded et qu'on n'a rien d'actif
                    if self.model.is_empty() {
                        if let Some(m) = self.available.iter().find(|m| m.loaded) {
                            self.model = m.id.clone();
                        }
                    }
                }
                Incoming::ModelLoaded(Ok(id)) => {
                    self.loading_model = None;
                    self.model = id;
                    self.refresh_models();
                }
                Incoming::ModelLoaded(Err(err)) => {
                    self.loading_model = None;
                    self.messages.push(Msg {
                        role: Role::Assistant,
                        content: format!("[Erreur chargement] {}", err),
                        reasoning: String::new(),
                        model: None,
                        tool_calls: Vec::new(),
                    });
                }
                Incoming::Token(tok) => {
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == Role::Assistant {
                            last.content.push_str(&tok);
                        }
                    }
                }
                Incoming::ReasoningToken(tok) => {
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == Role::Assistant {
                            last.reasoning.push_str(&tok);
                        }
                    }
                }
                Incoming::Usage { used, finish } => {
                    if let Some(p) = self.pending_stats.take() {
                        self.predictor
                            .record(&p.model, p.prompt_chars, p.allocated, used, &finish);
                    }
                }
                Incoming::StreamDone => {
                    self.waiting = false;
                    self.stream_handle = None;
                    if let Some(last) = self.messages.last() {
                        if last.role == Role::Assistant
                            && last.content.is_empty()
                            && last.reasoning.is_empty()
                        {
                            self.messages.pop();
                        }
                    }
                    // Score de sycophance sur le dernier message assistant (content uniquement,
                    // pas le reasoning — on juge ce que le modele produit a l'utilisateur).
                    if let Some(last) = self.messages.last() {
                        if last.role == Role::Assistant && !last.content.is_empty() {
                            if let Some(m) = &last.model {
                                let (score, flags) = score_sycophancy(&last.content);
                                self.syco.record(m, score, flags);
                            }
                        }
                    }
                    // Genere le thought flow Mermaid si des tools ont ete utilisees.
                    if let Some(last) = self.messages.last() {
                        if last.role == Role::Assistant && !last.tool_calls.is_empty() {
                            let user_prompt = self.messages.iter().rev()
                                .find(|m| m.role == Role::User)
                                .map(|m| m.content.as_str())
                                .unwrap_or("?");
                            let flow = generate_thought_flow(last, user_prompt);
                            save_thought_flow(&flow, &self.ai_workdir);
                            self.thought_flow = flow;
                            self.show_thought_flow = true;
                        }
                    }
                }
                Incoming::StreamError(err) => {
                    self.waiting = false;
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == Role::Assistant {
                            last.content = format!("[Erreur] {}", err);
                        }
                    }
                    // Sauver aussi le thought flow en erreur — c'est souvent le cas le plus utile a debugger.
                    if let Some(last) = self.messages.last() {
                        if last.role == Role::Assistant && !last.tool_calls.is_empty() {
                            let user_prompt = self.messages.iter().rev()
                                .find(|m| m.role == Role::User)
                                .map(|m| m.content.as_str())
                                .unwrap_or("?");
                            let flow = generate_thought_flow(last, user_prompt);
                            save_thought_flow(&flow, &self.ai_workdir);
                            self.thought_flow = flow;
                            self.show_thought_flow = true;
                        }
                    }
                }
                Incoming::ToolCallComplete(info) => {
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == Role::Assistant {
                            last.tool_calls.push(info);
                        }
                    }
                }
                Incoming::ToolLoopIteration(_n) => {
                    // Le streaming repart pour un nouveau tour.
                    // On ne cree PAS un nouveau Msg — les tokens vont dans le meme.
                }
            }
        }
    }

    fn show_predictor_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("predictor")
            .resizable(true)
            .default_width(240.0)
            .min_width(180.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.heading("🎯 Predicteur");
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui.small_button("✕").on_hover_text("Masquer").clicked() {
                                self.show_predictor = false;
                            }
                        },
                    );
                });
                ui.weak("V1 — EMA par bucket");

                // Bloc pedagogique deplace en haut, sous le titre.
                egui::CollapsingHeader::new("ℹ C'est quoi ce panneau ?")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "Predicteur V1 — il apprend combien de tokens \
                                 chaque modele utilise VRAIMENT selon la taille \
                                 du prompt.",
                            )
                            .small()
                            .italics(),
                        );
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Les 5 buckets").small().strong());
                        ui.label(egui::RichText::new("tiny   < 50 chars").small().monospace());
                        ui.label(egui::RichText::new("short  < 200 chars").small().monospace());
                        ui.label(egui::RichText::new("medium < 1000 chars").small().monospace());
                        ui.label(egui::RichText::new("long   < 4000 chars").small().monospace());
                        ui.label(egui::RichText::new("xlong  >= 4000 chars").small().monospace());
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Comment il apprend").small().strong());
                        ui.label(
                            egui::RichText::new(
                                "A chaque reponse : il note combien le modele a \
                                 vraiment consomme (completion_tokens). Il met a \
                                 jour une moyenne exponentielle (EMA = 0.7 x ancien \
                                 + 0.3 x nouveau) par bucket x modele.",
                            )
                            .small(),
                        );
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Comment il predit").small().strong());
                        ui.label(
                            egui::RichText::new(
                                "Prochain tir : alloue ema x 1.3 (marge 30%). \
                                 Si le modele a deja ete tronque (length_hits > 0) \
                                 sur ce bucket, il passe a ema x 1.8 (marge 80%) \
                                 pour compenser. Tant qu'il n'a pas de donnees \
                                 (n=0), il utilise un fallback fixe (fb).",
                            )
                            .small(),
                        );
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("A quoi ca sert").small().strong());
                        ui.label(
                            egui::RichText::new(
                                "Sans lui : tu alloues au pif (souvent trop = \
                                 reponses qui tournent en rond ; souvent pas assez \
                                 = reponses tronquees). Avec lui : l'app calibre \
                                 elle-meme chaque modele. Plus tu l'utilises, plus \
                                 c'est precis.",
                            )
                            .small(),
                        );
                        ui.add_space(4.0);
                    });
                ui.separator();

                // Scroll vertical pour tout le contenu dynamique.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                if self.model.is_empty() {
                    ui.weak("(aucun modele)");
                    return;
                }

                // Bucket courant
                let chars = self.input.chars().count();
                let cur_bucket = bucket_of(chars);
                let cur_pred = self.predictor.predict(&self.model, chars);
                ui.label(format!("Prompt : {} chars", chars));
                ui.label(format!(
                    "Bucket : {} ({})",
                    BUCKETS[cur_bucket].1, cur_bucket + 1
                ));
                ui.colored_label(
                    egui::Color32::from_rgb(255, 215, 0),
                    format!("Predit : {} tokens", cur_pred),
                );
                ui.add_space(8.0);
                ui.separator();

                // Table par bucket pour le modele actif
                ui.label(egui::RichText::new("Par bucket").strong());
                ui.add_space(4.0);
                for (i, (_up, name, fallback)) in BUCKETS.iter().enumerate() {
                    let key = (self.model.clone(), i);
                    let stats = self.predictor.table.get(&key);
                    let is_current = i == cur_bucket;
                    let frame_color = if is_current {
                        egui::Color32::from_rgb(40, 60, 90)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::default()
                        .fill(frame_color)
                        .inner_margin(egui::Margin::symmetric(4, 3))
                        .corner_radius(3)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(*name).monospace().small());
                                if let Some(s) = stats {
                                    let color = if s.length_hits > 0 {
                                        egui::Color32::from_rgb(255, 150, 80) // warning
                                    } else {
                                        egui::Color32::from_rgb(120, 220, 120) // ok
                                    };
                                    ui.colored_label(
                                        color,
                                        format!("ema {:.0}", s.ema),
                                    );
                                    ui.weak(format!("n={}", s.samples));
                                    if s.length_hits > 0 {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(255, 100, 100),
                                            format!("⚠{}", s.length_hits),
                                        );
                                    }
                                } else {
                                    ui.weak(format!("fb={}", fallback));
                                }
                            });
                            // Barre de dernier usage vs dernier alloue
                            if let Some(s) = stats {
                                if s.last_allocated > 0 {
                                    let ratio = (s.last_used as f32) / (s.last_allocated as f32);
                                    let ratio = ratio.clamp(0.0, 1.2);
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::Vec2::new(ui.available_width(), 6.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        1.0,
                                        egui::Color32::from_rgb(40, 45, 55),
                                    );
                                    let mut fill_rect = rect;
                                    fill_rect.set_width(rect.width() * ratio.min(1.0));
                                    let fill_color = if s.last_finish == "length" {
                                        egui::Color32::from_rgb(255, 100, 100)
                                    } else if ratio > 0.9 {
                                        egui::Color32::from_rgb(255, 200, 80)
                                    } else {
                                        egui::Color32::from_rgb(100, 200, 140)
                                    };
                                    ui.painter().rect_filled(fill_rect, 1.0, fill_color);
                                }
                            }
                        });
                    ui.add_space(2.0);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.weak("● vert = marge OK (budget confortable, le modele a fini large)");
                ui.weak("● orange = proche du max (>90% du budget utilise)");
                ui.weak("● rouge = budget atteint (finish_reason: length)");
                ui.weak("   → peut etre une coupure en plein reasoning OU en plein content.");
                ui.add_space(8.0);
                    });
            });
    }

    /// Section Workspace (arborescence + recherche) dessinee dans le panneau gauche unifie.
    fn draw_file_tree_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("🌳 Workspace");
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui.small_button("✕").on_hover_text("Masquer").clicked() {
                        self.show_file_tree = false;
                        self.persist_settings();
                    }
                    if ui.small_button("🔄").on_hover_text("Rafraichir").clicked() {
                        self.tree_cache.clear();
                        self.tree_search_cache = None;
                    }
                },
            );
        });

        // Racine editable
        ui.horizontal(|ui| {
            ui.label("📁");
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.workspace_path)
                    .hint_text(default_workspace().display().to_string())
                    .desired_width(ui.available_width()),
            );
            if r.changed() {
                self.tree_cache.clear();
                self.tree_search_cache = None;
                self.persist_settings();
            }
        });

        let root = self.workspace_root();
        if !root.is_dir() {
            ui.colored_label(
                egui::Color32::from_rgb(230, 120, 120),
                "⚠ racine introuvable",
            );
            return;
        }

        // Bandeau workdir IA (si defini)
        if !self.ai_workdir.trim().is_empty() {
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(50, 45, 20))
                .inner_margin(egui::Margin::symmetric(6, 4))
                .corner_radius(3)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("⭐ IA :")
                                .color(egui::Color32::from_rgb(255, 220, 100))
                                .strong(),
                        );
                        let wd = self.ai_workdir.clone();
                        ui.add(egui::Label::new(
                            egui::RichText::new(&wd)
                                .color(egui::Color32::from_rgb(255, 220, 100))
                                .small(),
                        )
                        .truncate())
                        .on_hover_text(&wd);
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.small_button("✕").on_hover_text("Effacer le workdir IA").clicked() {
                                    self.ai_workdir.clear();
                                    self.persist_settings();
                                }
                            },
                        );
                    });
                });
        }

        ui.label(
            egui::RichText::new("cliquez droit pour definir le repertoire de travail de l'ia.")
                .color(egui::Color32::from_rgb(255, 220, 100))
                .small(),
        );

        // Barre de recherche
        ui.horizontal(|ui| {
            ui.label("🔎");
            ui.add(
                egui::TextEdit::singleline(&mut self.tree_search)
                    .hint_text("rechercher (nom)…")
                    .desired_width(ui.available_width()),
            );
        });

        ui.separator();

        let search = self.tree_search.trim().to_string();
        egui::ScrollArea::vertical()
            .id_salt("tree_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if search.is_empty() {
                    self.draw_tree_dir(ui, &root, 0);
                } else {
                    self.draw_search_results(ui, &root, &search);
                }
            });
    }

    fn draw_search_results(&mut self, ui: &mut egui::Ui, root: &Path, query: &str) {
        // Cache : invalide si query a change ou > 3s
        let need = match &self.tree_search_cache {
            Some((q, _, t)) => q != query || t.elapsed() > Duration::from_secs(3),
            None => true,
        };
        if need {
            let results = search_recursive(root, query, 6, 300);
            self.tree_search_cache = Some((query.to_string(), results, Instant::now()));
        }
        let results = self
            .tree_search_cache
            .as_ref()
            .map(|(_, r, _)| r.clone())
            .unwrap_or_default();

        if results.is_empty() {
            ui.weak("(aucun resultat)");
            return;
        }
        ui.weak(format!("{} resultat(s) — max 300, profondeur 6", results.len()));
        ui.add_space(4.0);

        for e in &results {
            ui.horizontal(|ui| {
                let path_str = e.path.display().to_string();
                let is_workdir = e.is_dir && self.ai_workdir.trim() == path_str;
                let icon = if e.is_dir { "📁" } else { "📄" };
                let rel = e.path.strip_prefix(root).unwrap_or(&e.path).display().to_string();
                let prefix = if is_workdir { "⭐ " } else { "" };
                let mut rt = egui::RichText::new(format!("{}{} {}", prefix, icon, rel));
                if is_workdir {
                    rt = rt.color(egui::Color32::from_rgb(255, 220, 100)).strong();
                }
                let lbl = ui
                    .add(egui::Label::new(rt).truncate().sense(egui::Sense::click()))
                    .on_hover_text(if e.is_dir {
                        format!("{}\n(clic droit = definir comme workdir IA)", path_str)
                    } else {
                        path_str.clone()
                    });
                if lbl.clicked() {
                    ui.ctx().copy_text(path_str.clone());
                }
                if e.is_dir && lbl.secondary_clicked() {
                    if is_workdir {
                        self.ai_workdir.clear();
                        self.persist_settings();
                    } else {
                        self.pending_workdir = Some(path_str.clone());
                    }
                }
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if ui
                            .small_button("📋")
                            .on_hover_text("Copier le chemin absolu")
                            .clicked()
                        {
                            ui.ctx().copy_text(path_str.clone());
                        }
                    },
                );
            });
        }
    }

    fn draw_tree_dir(&mut self, ui: &mut egui::Ui, dir: &Path, depth: u32) {
        if depth > 12 {
            ui.weak("(profondeur max)");
            return;
        }
        let entries = self.tree_dir_entries(dir);
        if entries.is_empty() {
            ui.weak("(vide)");
            return;
        }
        for e in entries {
            if e.truncated {
                ui.weak(&e.name);
                continue;
            }
            if e.is_dir {
                let path_str = e.path.display().to_string();
                let is_workdir = self.ai_workdir.trim() == path_str;
                let header_text = if is_workdir {
                    egui::RichText::new(format!("⭐ 📁 {}", e.name))
                        .color(egui::Color32::from_rgb(255, 220, 100))
                        .strong()
                } else {
                    egui::RichText::new(format!("📁 {}", e.name))
                };
                let id = ui.make_persistent_id(("tree_dir", &e.path));
                let header = egui::CollapsingHeader::new(header_text)
                    .id_salt(id)
                    .default_open(false);
                let ch_resp = header.show(ui, |ui| {
                    let p = e.path.clone();
                    self.draw_tree_dir(ui, &p, depth + 1);
                });
                let hdr = ch_resp
                    .header_response
                    .on_hover_text("clic droit = definir comme workdir IA");
                if hdr.secondary_clicked() {
                    if is_workdir {
                        self.ai_workdir.clear();
                        self.persist_settings();
                    } else {
                        self.pending_workdir = Some(path_str);
                    }
                }
            } else {
                ui.horizontal(|ui| {
                    let path_str = e.path.display().to_string();
                    let label = format!("📄 {}", e.name);
                    if ui
                        .add(egui::Label::new(label).truncate())
                        .on_hover_text(&path_str)
                        .clicked()
                    {
                        ui.ctx().copy_text(path_str.clone());
                    }
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.weak(format_size(e.size));
                            if ui
                                .small_button("📋")
                                .on_hover_text("Copier le chemin absolu")
                                .clicked()
                            {
                                ui.ctx().copy_text(path_str.clone());
                            }
                        },
                    );
                });
            }
        }
    }

    /// Popup de confirmation avant de changer le workdir IA.
    fn show_workdir_confirm(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_workdir.clone() else {
            return;
        };
        let mut decision: Option<bool> = None; // Some(true) = oui, Some(false) = non
        egui::Window::new("Confirmer le workdir IA")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label("Vous etes actuellement sur :");
                ui.add_space(4.0);
                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(40, 45, 20))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .corner_radius(3)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&pending)
                                .color(egui::Color32::from_rgb(255, 220, 100))
                                .monospace(),
                        );
                    });
                ui.add_space(8.0);
                let current = if self.ai_workdir.trim().is_empty() {
                    "(aucun)".to_string()
                } else {
                    self.ai_workdir.clone()
                };
                ui.horizontal(|ui| {
                    ui.weak("Workdir actuel :");
                    ui.weak(current);
                });
                ui.add_space(10.0);
                ui.label("Etes-vous sur de travailler sur ce repertoire ?");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_sized(
                            [100.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new("✔ Oui")
                                    .color(egui::Color32::from_rgb(180, 240, 180))
                                    .strong(),
                            ),
                        )
                        .clicked()
                    {
                        decision = Some(true);
                    }
                    if ui
                        .add_sized(
                            [100.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new("✖ Non")
                                    .color(egui::Color32::from_rgb(240, 180, 180)),
                            ),
                        )
                        .clicked()
                    {
                        decision = Some(false);
                    }
                });
                ui.add_space(2.0);
            });

        match decision {
            Some(true) => {
                self.ai_workdir = pending;
                self.pending_workdir = None;
                self.persist_settings();
            }
            Some(false) => {
                self.pending_workdir = None;
            }
            None => {}
        }
    }

    /// Panneau gauche unifie : Sycometer (haut) + Workspace (bas).
    /// Ne s'affiche que si au moins une des deux sections est activee.
    fn show_left_panel(&mut self, ctx: &egui::Context) {
        if !self.show_syco && !self.show_file_tree {
            return;
        }
        egui::SidePanel::left("left_tools")
            .resizable(true)
            .default_width(260.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                if self.show_syco {
                    self.draw_syco_section(ui);
                }
                if self.show_syco && self.show_file_tree {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                if self.show_file_tree {
                    self.draw_file_tree_section(ui);
                }
            });
    }

    /// Section Sycometer dessinee dans le panneau gauche unifie.
    fn draw_syco_section(&mut self, ui: &mut egui::Ui) {
                ui.horizontal(|ui| {
                    ui.heading("🎭 Sycometer");
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui.small_button("✕").on_hover_text("Masquer").clicked() {
                                self.show_syco = false;
                                self.persist_settings();
                            }
                        },
                    );
                });
                ui.weak("% de flatterie par modele");
                ui.separator();

                if self.syco.table.is_empty() {
                    ui.weak("(aucune mesure pour l'instant)");
                    ui.add_space(4.0);
                    ui.weak("Envoie un message, je note");
                    ui.weak("a quel point il te lèche.");
                    return;
                }

                // Tri : modele actif en haut, puis par EMA decroissante
                let mut entries: Vec<(String, SycoStats)> = self
                    .syco
                    .table
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                entries.sort_by(|a, b| {
                    let a_cur = a.0 == self.model;
                    let b_cur = b.0 == self.model;
                    match (a_cur, b_cur) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => b.1.ema.partial_cmp(&a.1.ema).unwrap_or(std::cmp::Ordering::Equal),
                    }
                });

                for (model, stats) in &entries {
                    let is_current = *model == self.model;
                    let frame_color = if is_current {
                        egui::Color32::from_rgb(60, 40, 80)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::default()
                        .fill(frame_color)
                        .inner_margin(egui::Margin::symmetric(6, 4))
                        .corner_radius(3)
                        .show(ui, |ui| {
                            // Nom du modele (tronque si trop long)
                            let short = if model.len() > 30 {
                                format!("{}…", &model[..29])
                            } else {
                                model.clone()
                            };
                            ui.label(egui::RichText::new(short).strong().small());

                            // Barre EMA
                            let pct = stats.ema.clamp(0.0, 100.0);
                            let (rect, _) = ui.allocate_exact_size(
                                egui::Vec2::new(ui.available_width(), 14.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                2.0,
                                egui::Color32::from_rgb(40, 45, 55),
                            );
                            let mut fill = rect;
                            fill.set_width(rect.width() * (pct / 100.0));
                            let fill_color = syco_color(pct);
                            ui.painter().rect_filled(fill, 2.0, fill_color);
                            // Pourcentage ecrit par-dessus
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("{:.0}%", pct),
                                egui::FontId::monospace(11.0),
                                egui::Color32::WHITE,
                            );

                            ui.horizontal(|ui| {
                                ui.weak(format!("n={}", stats.samples));
                                ui.weak(format!("dernier : {:.0}%", stats.last_score));
                            });

                            // Flags du dernier message (max 5)
                            if !stats.last_flags.is_empty() {
                                let preview: Vec<String> =
                                    stats.last_flags.iter().take(5).cloned().collect();
                                let more = if stats.last_flags.len() > 5 {
                                    format!(" +{}", stats.last_flags.len() - 5)
                                } else {
                                    String::new()
                                };
                                ui.weak(format!("→ {}{}", preview.join(", "), more));
                            }
                        });
                    ui.add_space(4.0);
                }

                ui.add_space(6.0);
                ui.separator();
                ui.weak("● < 20 : direct");
                ui.weak("● 20-50 : poli");
                ui.weak("● 50-75 : flatteur");
                ui.weak("● > 75 : lèche-bottes");
    }


    fn show_persona(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("🎩 Persona");
                ui.weak("— le role que le modele incarne pendant toute la conversation");
            });
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // ── Champ System Prompt ─────────────────────────────────────
                    let sys_header_color = if self.system_prompt.trim().is_empty() {
                        egui::Color32::from_rgb(140, 140, 140)
                    } else {
                        egui::Color32::from_rgb(255, 200, 120)
                    };
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(28, 22, 18))
                        .inner_margin(egui::Margin::symmetric(14, 12))
                        .corner_radius(5)
                        .show(ui, |ui| {
                            let title = if self.system_prompt.trim().is_empty() {
                                "🎩 Ton system prompt (inactif)".to_string()
                            } else {
                                format!(
                                    "🎩 Ton system prompt ({} chars, actif)",
                                    self.system_prompt.chars().count()
                                )
                            };
                            ui.label(
                                egui::RichText::new(title)
                                    .heading()
                                    .color(sys_header_color),
                            );
                            ui.add_space(4.0);
                            ui.weak(
                                "Persona = qui est l'IA, ses competences, ses limites. \
                                 Un role durable : \"Tu es un ingenieur Rust senior, direct, pas de flatterie\". \
                                 Reste sobre — precis vaut mieux que grandiloquent. Vide = aucun system prompt envoye.",
                            );
                            ui.add_space(4.0);
                            ui.colored_label(
                                egui::Color32::from_rgb(130, 180, 140),
                                "💡 Pour une instruction ponctuelle (format, langue, longueur), \
                                 mets-la directement dans ton message Chat — pas ici. Le Persona \
                                 est pour ce qui dure TOUTE la conversation.",
                            );
                            ui.add_space(8.0);

                            let resp = ui.add(
                                egui::TextEdit::multiline(&mut self.system_prompt)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(8)
                                    .hint_text(
                                        "Tu es un ingenieur Rust senior. Tu es direct, pas de flatterie...",
                                    ),
                            );
                            if resp.changed() {
                                save_system_prompt(&self.system_prompt);
                            }

                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.weak(format!("{} chars", self.system_prompt.chars().count()));
                                if ui
                                    .add_enabled(
                                        !self.system_prompt.trim().is_empty(),
                                        egui::Button::new("→ Tester dans le Chat"),
                                    )
                                    .on_hover_text(
                                        "Bascule sur l'onglet Chat. Ta persona est active : tape un message pour tester.",
                                    )
                                    .clicked()
                                {
                                    ui.ctx().memory_mut(|m| {
                                        m.data.insert_temp(
                                            egui::Id::new("lab_switch_to_chat"),
                                            true,
                                        )
                                    });
                                }
                                if ui
                                    .add_enabled(
                                        !self.system_prompt.is_empty(),
                                        egui::Button::new("🗑 Vider"),
                                    )
                                    .on_hover_text("Retire le system prompt (il ne sera plus envoye)")
                                    .clicked()
                                {
                                    self.system_prompt.clear();
                                    save_system_prompt("");
                                }
                                if ui
                                    .add_enabled(
                                        !self.system_prompt.trim().is_empty(),
                                        egui::Button::new("📋 Copier"),
                                    )
                                    .clicked()
                                {
                                    ui.ctx().copy_text(self.system_prompt.clone());
                                }
                                ui.weak(format!(
                                    "Fichier : {}",
                                    system_prompt_path().display()
                                ));
                            });
                        });

                    ui.add_space(14.0);

                    // ── Bibliotheque d'exemples de personas ─────────────────────
                    let persona_cat = &PROMPT_CATEGORIES[0];
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(24, 24, 32))
                        .inner_margin(egui::Margin::symmetric(14, 12))
                        .corner_radius(5)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("📚  Exemples de personas pretes a charger")
                                    .heading()
                                    .color(egui::Color32::from_rgb(153, 220, 255)),
                            );
                            ui.add_space(6.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(persona_cat.description)
                                        .italics()
                                        .color(egui::Color32::from_rgb(190, 190, 190)),
                                )
                                .wrap(),
                            );
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(6.0);

                            for (idx, ex) in persona_cat.examples.iter().enumerate() {
                                egui::Frame::default()
                                    .fill(egui::Color32::from_rgb(30, 32, 40))
                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                    .corner_radius(4)
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}. {}",
                                                idx + 1,
                                                ex.title
                                            ))
                                            .strong()
                                            .color(egui::Color32::from_rgb(255, 200, 120)),
                                        );
                                        ui.add_space(4.0);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(ex.explanation)
                                                    .italics()
                                                    .color(egui::Color32::from_rgb(180, 180, 180)),
                                            )
                                            .wrap(),
                                        );
                                        ui.add_space(6.0);
                                        egui::Frame::default()
                                            .fill(egui::Color32::from_rgb(20, 22, 28))
                                            .inner_margin(egui::Margin::symmetric(8, 6))
                                            .corner_radius(3)
                                            .show(ui, |ui| {
                                                let mut t = ex.template.to_string();
                                                ui.add(
                                                    egui::TextEdit::multiline(&mut t)
                                                        .font(egui::TextStyle::Monospace)
                                                        .desired_width(f32::INFINITY)
                                                        .desired_rows(
                                                            ex.template
                                                                .lines()
                                                                .count()
                                                                .min(10)
                                                                .max(3),
                                                        )
                                                        .interactive(true),
                                                );
                                            });
                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            if ui
                                                .button("🎩 Charger comme system")
                                                .on_hover_text(
                                                    "Installe ce template comme System Prompt actif.",
                                                )
                                                .clicked()
                                            {
                                                let payload = ex.template.to_string();
                                                ui.ctx().memory_mut(|m| {
                                                    m.data.insert_temp(
                                                        egui::Id::new("lab_load_as_system"),
                                                        payload,
                                                    )
                                                });
                                            }
                                            if ui.button("📋 Copier").clicked() {
                                                ui.ctx().copy_text(ex.template.to_string());
                                            }
                                        });
                                    });
                                ui.add_space(6.0);
                            }
                            ui.add_space(8.0);
                        });
                    ui.add_space(16.0);
                });
        });

        // Handler : charger un template comme system prompt (depuis biblio Personas).
        let to_system: Option<String> = ctx.memory_mut(|m| {
            m.data.get_temp::<String>(egui::Id::new("lab_load_as_system"))
        });
        if let Some(text) = to_system {
            ctx.memory_mut(|m| m.data.remove_temp::<String>(egui::Id::new("lab_load_as_system")));
            self.system_prompt = text;
            save_system_prompt(&self.system_prompt);
        }

        // Handler : bouton "Tester dans le Chat".
        let switch: Option<bool> = ctx.memory_mut(|m| {
            m.data.get_temp::<bool>(egui::Id::new("lab_switch_to_chat"))
        });
        if switch.is_some() {
            ctx.memory_mut(|m| m.data.remove_temp::<bool>(egui::Id::new("lab_switch_to_chat")));
            self.view = View::Chat;
        }
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("⚙  Parametres");
                ui.weak("— preferences persistees dans settings.json a cote de l'exe");
            });
            ui.add_space(12.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // ── Panneaux lateraux ──────────────────────────────────────
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(22, 26, 34))
                        .inner_margin(egui::Margin::symmetric(14, 12))
                        .corner_radius(5)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Panneaux lateraux (dans l'onglet Chat)")
                                    .heading()
                                    .color(egui::Color32::from_rgb(180, 200, 255)),
                            );
                            ui.add_space(6.0);
                            let mut p = self.show_predictor;
                            if ui
                                .checkbox(&mut p, "🎯 Predicteur — EMA par bucket (droite)")
                                .on_hover_text(
                                    "Affiche un panneau qui apprend combien de tokens \
                                     chaque modele utilise reellement par palier.",
                                )
                                .changed()
                            {
                                self.show_predictor = p;
                                self.persist_settings();
                            }
                            let mut s = self.show_syco;
                            if ui
                                .checkbox(&mut s, "🎭 Sycometer — % de flatterie par modele (gauche)")
                                .on_hover_text(
                                    "Affiche un panneau qui mesure le taux de flatterie (EMA) \
                                     par modele dans ses reponses.",
                                )
                                .changed()
                            {
                                self.show_syco = s;
                                self.persist_settings();
                            }
                            let mut t = self.show_file_tree;
                            if ui
                                .checkbox(&mut t, "🌳 Arborescence fichiers (gauche)")
                                .on_hover_text(
                                    "Affiche un panneau explorateur du repertoire de travail : \
                                     arbo lazy, recherche, copier-coller de chemins.",
                                )
                                .changed()
                            {
                                self.show_file_tree = t;
                                self.persist_settings();
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label("📁 Racine workspace :");
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut self.workspace_path)
                                        .hint_text(default_workspace().display().to_string())
                                        .desired_width(ui.available_width() - 80.0),
                                );
                                if r.changed() {
                                    self.tree_cache.clear();
                                    self.tree_search_cache = None;
                                    self.persist_settings();
                                }
                                if ui.small_button("↺").on_hover_text("Defaut (exe)").clicked() {
                                    self.workspace_path.clear();
                                    self.tree_cache.clear();
                                    self.tree_search_cache = None;
                                    self.persist_settings();
                                }
                            });
                            ui.weak(
                                "Vide = dossier parent de l'executable. Utilise par le panneau arborescence.",
                            );
                        });

                    ui.add_space(10.0);

                    // ── Modele par defaut ──────────────────────────────────────
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(22, 26, 34))
                        .inner_margin(egui::Margin::symmetric(14, 12))
                        .corner_radius(5)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Modele — defauts au demarrage")
                                    .heading()
                                    .color(egui::Color32::from_rgb(180, 200, 255)),
                            );
                            ui.add_space(6.0);
                            ui.weak(
                                "Ces valeurs sont chargees au lancement de l'app. \
                                 Tu peux les changer en live dans la barre du haut (onglet Chat).",
                            );
                            ui.add_space(6.0);
                            let mut r = self.reasoning_enabled;
                            if ui
                                .checkbox(&mut r, "🧠 Reasoning active par defaut")
                                .changed()
                            {
                                self.reasoning_enabled = r;
                                self.persist_settings();
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label("Max tokens par defaut :");
                                let preview = if self.max_tokens == 0 {
                                    "Auto".to_string()
                                } else {
                                    format!("{}", self.max_tokens)
                                };
                                let prev = self.max_tokens;
                                egui::ComboBox::from_id_salt("settings_max_tokens")
                                    .selected_text(preview)
                                    .width(120.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.max_tokens, 0, "Auto");
                                        ui.separator();
                                        for n in [512u32, 1024, 2048, 4096, 8192, 16384] {
                                            ui.selectable_value(
                                                &mut self.max_tokens,
                                                n,
                                                format!("{}", n),
                                            );
                                        }
                                    });
                                if self.max_tokens != prev {
                                    self.persist_settings();
                                }
                            });
                        });

                    ui.add_space(10.0);

                    // ── Sampling (parametres modele envoyes a LM Studio) ─────
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(22, 26, 34))
                        .inner_margin(egui::Margin::symmetric(14, 12))
                        .corner_radius(5)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Sampling — envoye dans chaque requete")
                                    .heading()
                                    .color(egui::Color32::from_rgb(180, 200, 255)),
                            );
                            ui.add_space(6.0);
                            ui.weak(
                                "Controle comment le modele choisit ses tokens. \
                                 Reste sur les defauts si tu ne sais pas, c'est tres bien.",
                            );
                            ui.add_space(8.0);

                            // Temperature
                            ui.horizontal(|ui| {
                                ui.label("🌡 Temperature :")
                                    .on_hover_text(
                                        "0.0 = deterministe (meme reponse a chaque fois). \
                                         1.0 = creatif standard. 2.0 = tres aleatoire.",
                                    );
                                let prev = self.temperature;
                                ui.add(
                                    egui::Slider::new(&mut self.temperature, 0.0..=2.0)
                                        .step_by(0.05)
                                        .fixed_decimals(2),
                                );
                                if (self.temperature - prev).abs() > f32::EPSILON {
                                    self.persist_settings();
                                }
                            });

                            // Top-P
                            ui.horizontal(|ui| {
                                ui.label("🎲 Top-p :")
                                    .on_hover_text(
                                        "Nucleus sampling. 1.0 = tous les tokens. 0.9 = seulement \
                                         les tokens qui couvrent 90% de la probabilite cumulee. \
                                         Reduit les choix improbables sans brider la creativite.",
                                    );
                                let prev = self.top_p;
                                ui.add(
                                    egui::Slider::new(&mut self.top_p, 0.0..=1.0)
                                        .step_by(0.01)
                                        .fixed_decimals(2),
                                );
                                if (self.top_p - prev).abs() > f32::EPSILON {
                                    self.persist_settings();
                                }
                            });

                            // Frequency penalty
                            ui.horizontal(|ui| {
                                ui.label("🔁 Frequency penalty :")
                                    .on_hover_text(
                                        "Penalise les mots deja utilises en fonction de leur \
                                         frequence. 0 = desactive, > 0 = reduit la repetition.",
                                    );
                                let prev = self.frequency_penalty;
                                ui.add(
                                    egui::Slider::new(&mut self.frequency_penalty, -2.0..=2.0)
                                        .step_by(0.05)
                                        .fixed_decimals(2),
                                );
                                if (self.frequency_penalty - prev).abs() > f32::EPSILON {
                                    self.persist_settings();
                                }
                            });

                            // Presence penalty
                            ui.horizontal(|ui| {
                                ui.label("👁 Presence penalty :")
                                    .on_hover_text(
                                        "Penalise les mots deja presents (binaire, peu importe \
                                         leur frequence). Encourage a introduire de nouveaux sujets.",
                                    );
                                let prev = self.presence_penalty;
                                ui.add(
                                    egui::Slider::new(&mut self.presence_penalty, -2.0..=2.0)
                                        .step_by(0.05)
                                        .fixed_decimals(2),
                                );
                                if (self.presence_penalty - prev).abs() > f32::EPSILON {
                                    self.persist_settings();
                                }
                            });

                            ui.add_space(6.0);

                            // Seed
                            ui.horizontal(|ui| {
                                let mut has_seed = self.seed.is_some();
                                if ui
                                    .checkbox(&mut has_seed, "🎯 Seed fixe")
                                    .on_hover_text(
                                        "Active : meme prompt + meme seed = meme reponse \
                                         (reproductibilite). Desactive : reponse aleatoire \
                                         a chaque fois (comportement normal).",
                                    )
                                    .changed()
                                {
                                    self.seed = if has_seed { Some(42) } else { None };
                                    self.persist_settings();
                                }
                                if let Some(ref mut s) = self.seed {
                                    let prev = *s;
                                    ui.add(
                                        egui::DragValue::new(s)
                                            .speed(1.0)
                                            .range(0..=i64::MAX),
                                    );
                                    if *s != prev {
                                        self.persist_settings();
                                    }
                                }
                            });

                            ui.add_space(6.0);

                            if ui
                                .button("↺ Reset defauts (0.7 / 1.0 / 0.0 / 0.0 / no-seed)")
                                .on_hover_text("Restaure les valeurs par defaut de sampling.")
                                .clicked()
                            {
                                self.temperature = 0.7;
                                self.top_p = 1.0;
                                self.frequency_penalty = 0.0;
                                self.presence_penalty = 0.0;
                                self.seed = None;
                                self.persist_settings();
                            }
                        });

                    ui.add_space(10.0);

                    // ── Info fichier ──────────────────────────────────────────
                    ui.weak(format!(
                        "Fichier settings : {}",
                        settings_path().display()
                    ));
                });
        });
    }

    fn show_chat(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("input").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // Bouton clear (toujours visible, disabled pendant generation)
                let clear_btn = ui.add_enabled(
                    !self.waiting && !self.messages.is_empty(),
                    egui::Button::new("🗑"),
                ).on_hover_text("Nouvelle conversation (efface l'historique)");
                if clear_btn.clicked() {
                    self.clear_conversation();
                }
                let has_newline = self.input.contains('\n');
                let desired_h = if has_newline { 80.0 } else { 30.0 };
                let desired_rows = if has_newline { 4 } else { 1 };
                let resp = ui.add_sized(
                    [ui.available_width() - 90.0, desired_h],
                    egui::TextEdit::multiline(&mut self.input)
                        .hint_text("Tape un message... (Enter envoyer, Shift+Enter nouvelle ligne)")
                        .desired_rows(desired_rows),
                );
                // Enter sans Shift = envoyer, Shift+Enter = nouvelle ligne
                let enter_pressed = resp.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                if self.waiting {
                    let stop_btn = ui.add(
                        egui::Button::new("⏹ Stop").fill(egui::Color32::from_rgb(140, 50, 50)),
                    );
                    if stop_btn.clicked() {
                        self.stop_generation();
                    }
                } else {
                    let send_btn = ui.button("Envoyer");
                    if send_btn.clicked() || enter_pressed {
                        self.send_message();
                        resp.request_focus();
                    }
                }
            });
            ui.add_space(4.0);
        });

        let mut action_set_max_tokens: Option<u32> = None;
        let mut action_disable_reasoning = false;
        let mut action_retry = false;
        let mut last_msg_truncated = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for (idx, m) in self.messages.iter().enumerate() {
                        let (label, color) = match m.role {
                            Role::User => (
                                "Toi".to_string(),
                                egui::Color32::from_rgb(102, 178, 255),
                            ),
                            Role::Assistant => (
                                m.model.clone().unwrap_or_else(|| "Bot".to_string()),
                                egui::Color32::from_rgb(153, 255, 153),
                            ),
                        };
                        ui.colored_label(color, label);

                        // ── Raisonnement (bulle grise separee, pliable) ────────
                        if !m.reasoning.is_empty() {
                            egui::Frame::default()
                                .fill(egui::Color32::from_rgb(26, 28, 34))
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .corner_radius(4)
                                .show(ui, |ui| {
                                    let header = format!(
                                        "🧠 Raisonnement  ({} chars)",
                                        m.reasoning.chars().count()
                                    );
                                    egui::CollapsingHeader::new(
                                        egui::RichText::new(header)
                                            .small()
                                            .color(egui::Color32::from_rgb(150, 150, 150)),
                                    )
                                    .id_salt(("reasoning_collapse", idx))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        let txt = egui::RichText::new(&m.reasoning)
                                            .italics()
                                            .color(egui::Color32::from_rgb(150, 150, 150));
                                        ui.add(
                                            egui::Label::new(txt).selectable(true).wrap(),
                                        );
                                    });
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .small_button("📋 Copier le raisonnement")
                                            .clicked()
                                        {
                                            ui.ctx().copy_text(m.reasoning.clone());
                                        }
                                    });
                                });
                            ui.add_space(4.0);
                        }

                        // ── Tool calls (bulles orange, pliables) ──────────────
                        for (tc_idx, tc) in m.tool_calls.iter().enumerate() {
                            egui::Frame::default()
                                .fill(egui::Color32::from_rgb(35, 30, 18))
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .corner_radius(4)
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    if tc.is_error {
                                        egui::Color32::from_rgb(200, 80, 80)
                                    } else {
                                        egui::Color32::from_rgb(180, 140, 60)
                                    },
                                ))
                                .show(ui, |ui| {
                                    let status = if tc.is_error { "❌" } else { "✅" };
                                    ui.label(
                                        egui::RichText::new(format!("🔧 {} {}", tc.name, status))
                                            .strong()
                                            .color(egui::Color32::from_rgb(255, 200, 100)),
                                    );
                                    // Arguments (monospace, compact)
                                    if !tc.arguments.is_empty() {
                                        let args_preview: String = tc.arguments.chars().take(200).collect();
                                        ui.label(
                                            egui::RichText::new(&args_preview)
                                                .monospace()
                                                .small()
                                                .color(egui::Color32::from_rgb(170, 170, 170)),
                                        );
                                    }
                                    // Resultat (pliable)
                                    let result_preview = if tc.result.len() > 60 {
                                        format!("{}...", &tc.result[..60])
                                    } else {
                                        tc.result.clone()
                                    };
                                    egui::CollapsingHeader::new(
                                        egui::RichText::new(format!("Resultat : {}", result_preview))
                                            .small()
                                            .color(if tc.is_error {
                                                egui::Color32::from_rgb(255, 140, 140)
                                            } else {
                                                egui::Color32::from_rgb(160, 200, 160)
                                            }),
                                    )
                                    .id_salt(("tool_result", idx, tc_idx))
                                    .default_open(false)
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&tc.result)
                                                    .monospace()
                                                    .small()
                                                    .color(egui::Color32::from_rgb(200, 210, 220)),
                                            )
                                            .selectable(true)
                                            .wrap(),
                                        );
                                    });
                                });
                            ui.add_space(4.0);
                        }

                        // ── Reponse (bulle blanche, mise en avant) ─────────────
                        if !m.content.is_empty() {
                            egui::Frame::default()
                                .fill(egui::Color32::from_rgb(22, 26, 32))
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .corner_radius(4)
                                .show(ui, |ui| {
                                    if m.role == Role::Assistant {
                                        ui.label(
                                            egui::RichText::new("💬 Reponse")
                                                .small()
                                                .color(egui::Color32::from_rgb(120, 180, 255)),
                                        );
                                        ui.add_space(2.0);
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&m.content)
                                                .color(egui::Color32::from_rgb(235, 240, 250)),
                                        )
                                        .selectable(true)
                                        .wrap(),
                                    );
                                    if m.role == Role::Assistant {
                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            if ui
                                                .small_button("📋 Copier la reponse")
                                                .clicked()
                                            {
                                                ui.ctx().copy_text(m.content.clone());
                                            }
                                            if !m.reasoning.is_empty() {
                                                if ui
                                                    .small_button("📋 Copier le raisonnement")
                                                    .clicked()
                                                {
                                                    ui.ctx().copy_text(m.reasoning.clone());
                                                }
                                                if ui
                                                    .small_button("📋 Copier raisonnement + reponse")
                                                    .on_hover_text(
                                                        "Copie le raisonnement puis la reponse, separes par une ligne.",
                                                    )
                                                    .clicked()
                                                {
                                                    let combined = format!(
                                                        "--- RAISONNEMENT ---\n{}\n\n--- REPONSE ---\n{}",
                                                        m.reasoning, m.content
                                                    );
                                                    ui.ctx().copy_text(combined);
                                                }
                                            }
                                        });
                                    }
                                });
                        } else if m.role == Role::Assistant
                            && !m.reasoning.is_empty()
                            && !self.waiting
                        {
                            // Cas Qwen qui meurt en reasoning sans jamais sortir de content.
                            egui::Frame::default()
                                .fill(egui::Color32::from_rgb(50, 30, 20))
                                .inner_margin(egui::Margin::symmetric(12, 10))
                                .corner_radius(4)
                                .stroke(egui::Stroke::new(
                                    2.0,
                                    egui::Color32::from_rgb(255, 130, 90),
                                ))
                                .show(ui, |ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(255, 180, 130),
                                        egui::RichText::new("⚠ Aucune reponse produite")
                                            .strong(),
                                    );
                                    ui.add_space(2.0);
                                    ui.weak(
                                        "Le modele a consomme tout son budget en raisonnement \
                                         avant de produire une reponse. Choisis une action :",
                                    );
                                    ui.add_space(6.0);
                                    ui.horizontal_wrapped(|ui| {
                                        if ui
                                            .button(
                                                egui::RichText::new("🔧 Max tokens → 8192")
                                                    .color(egui::Color32::from_rgb(255, 215, 0)),
                                            )
                                            .on_hover_text("Double le budget. Suffisant pour la plupart des reasoning models.")
                                            .clicked()
                                        {
                                            action_set_max_tokens = Some(8192);
                                        }
                                        if ui
                                            .button(
                                                egui::RichText::new("🔧 Max tokens → 16384")
                                                    .color(egui::Color32::from_rgb(255, 215, 0)),
                                            )
                                            .on_hover_text("Budget maximum. Utile si 8192 ne suffit toujours pas.")
                                            .clicked()
                                        {
                                            action_set_max_tokens = Some(16384);
                                        }
                                        if ui
                                            .button(
                                                egui::RichText::new("⚡ Desactiver Reasoning")
                                                    .color(egui::Color32::from_rgb(200, 230, 255)),
                                            )
                                            .on_hover_text(
                                                "Force la reponse directe. Certains modeles l'ignorent mais tentent au moins.",
                                            )
                                            .clicked()
                                        {
                                            action_disable_reasoning = true;
                                        }
                                    });
                                    ui.add_space(6.0);
                                    ui.separator();
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_enabled(
                                                !self.waiting,
                                                egui::Button::new(
                                                    egui::RichText::new("🔁  Reiterer la requete")
                                                        .color(egui::Color32::from_rgb(180, 255, 180))
                                                        .strong(),
                                                ),
                                            )
                                            .on_hover_text(
                                                "Renvoie la meme question au modele avec les parametres courants (max_tokens, reasoning, system_prompt). Le message foireux est remplace.",
                                            )
                                            .clicked()
                                        {
                                            action_retry = true;
                                        }
                                        ui.weak("— applique d'abord un des boutons ci-dessus, puis reitere.");
                                    });
                                });
                            last_msg_truncated = true;
                        }
                        ui.add_space(10.0);
                    }
                    if self.waiting {
                        ui.weak("...");
                    }
                });
        });

        // Applique les actions issues des boutons du warning (hors iter).
        if let Some(n) = action_set_max_tokens {
            self.max_tokens = n;
            self.persist_settings();
        }
        if action_disable_reasoning {
            self.reasoning_enabled = false;
            self.persist_settings();
        }
        if action_retry {
            self.retry_last_assistant();
        }
        // Expose l'etat tronque pour la barre du haut (highlight ComboBox).
        self.last_truncated = last_msg_truncated;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_incoming();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // Barre de navigation : Chat / Persona / [Prompts avances] / Settings
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.view == View::Chat, "💬  Chat")
                    .clicked()
                {
                    self.view = View::Chat;
                }
                if ui
                    .selectable_label(self.view == View::Persona, "🎩  Persona")
                    .clicked()
                {
                    self.view = View::Persona;
                }
                if ui
                    .selectable_label(self.view == View::Settings, "⚙  Parametres")
                    .clicked()
                {
                    self.view = View::Settings;
                }
                // Badge System Prompt (a droite)
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if !self.system_prompt.trim().is_empty() {
                            let preview: String = self
                                .system_prompt
                                .chars()
                                .take(80)
                                .collect::<String>()
                                .replace('\n', " ");
                            let label = egui::RichText::new("🎩 System actif")
                                .color(egui::Color32::from_rgb(255, 200, 120));
                            let resp = ui.label(label);
                            resp.on_hover_text(format!(
                                "System prompt actif ({} chars) :\n\n{}{}",
                                self.system_prompt.chars().count(),
                                preview,
                                if self.system_prompt.chars().count() > 80 {
                                    "..."
                                } else {
                                    ""
                                },
                            ));
                            if ui
                                .small_button("→ Persona")
                                .on_hover_text("Ouvrir l'onglet Persona pour editer le system prompt")
                                .clicked()
                            {
                                self.view = View::Persona;
                            }
                        } else {
                            ui.weak("🎩 no system");
                        }
                    },
                );
            });
            ui.add_space(2.0);
        });

        // Barre du haut : modele actif + toggle reasoning
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let current = if self.model.is_empty() {
                    "Aucun modele".to_string()
                } else {
                    self.model.clone()
                };
                let mut pick: Option<String> = None;
                egui::ComboBox::from_id_salt("model_picker")
                    .selected_text(current)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        if self.available.is_empty() {
                            ui.weak("(aucun modele — LM Studio lance ?)");
                        }
                        for m in &self.available {
                            let dot = if m.loaded { "●" } else { "○" };
                            let label = format!("{}  {}", dot, m.id);
                            let color = if m.loaded {
                                egui::Color32::from_rgb(127, 212, 127)
                            } else {
                                ui.visuals().text_color()
                            };
                            if ui
                                .selectable_label(
                                    self.model == m.id,
                                    egui::RichText::new(label).color(color),
                                )
                                .clicked()
                            {
                                pick = Some(m.id.clone());
                            }
                        }
                        ui.separator();
                        if ui.button("⟳ Rafraichir").clicked() {
                            pick = Some("__refresh__".to_string());
                        }
                    });
                if let Some(id) = pick {
                    if id == "__refresh__" {
                        self.refresh_models();
                    } else if let Some(m) = self.available.iter().find(|m| m.id == id) {
                        if m.loaded {
                            self.model = id;
                        } else {
                            self.request_load(id);
                        }
                    }
                }
                if let Some(loading) = &self.loading_model {
                    ui.spinner();
                    ui.weak(format!("Chargement {}...", loading));
                }
                ui.separator();
                ui.weak("tokens");
                let preview = if self.max_tokens == 0 {
                    // Apercu live : utilise le predicteur si on a un modele, sinon heuristique
                    let chars = self.input.chars().count();
                    let n = if self.model.is_empty() {
                        auto_max_tokens(chars)
                    } else {
                        self.predictor.predict(&self.model, chars)
                    };
                    format!("Auto ({})", n)
                } else {
                    format!("{}", self.max_tokens)
                };
                let max_tokens_stroke = if self.last_truncated {
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 130, 90))
                } else {
                    egui::Stroke::NONE
                };
                egui::Frame::default()
                    .stroke(max_tokens_stroke)
                    .corner_radius(4)
                    .inner_margin(egui::Margin::symmetric(2, 0))
                    .show(ui, |ui| {
                        egui::ComboBox::from_id_salt("max_tokens")
                            .selected_text(preview)
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.max_tokens, 0, "Auto");
                                ui.separator();
                                for n in [512u32, 1024, 2048, 4096, 8192, 16384] {
                                    ui.selectable_value(
                                        &mut self.max_tokens,
                                        n,
                                        format!("{}", n),
                                    );
                                }
                            })
                            .response
                            .on_hover_text(
                                "Auto : paliers selon la longueur du prompt (1024 a 16384).\n\
                                 Sinon : valeur fixe (reasoning + content confondus).",
                            );
                    });
                if self.last_truncated {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 130, 90),
                        "← augmenter ici",
                    );
                }
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let icon = if self.reasoning_enabled { "🧠" } else { "⚡" };
                        let resp = ui.toggle_value(
                            &mut self.reasoning_enabled,
                            format!("{} Reasoning", icon),
                        );
                        let tip = if self.reasoning_enabled {
                            "Le modele reflechit avant de repondre (gris italique)."
                        } else {
                            "Reponse directe tentee. Certains modeles forcent le reasoning quand meme."
                        };
                        resp.on_hover_text(tip);
                        ui.separator();
                        let tool_icon = if self.tools_enabled { "🔧" } else { "🚫" };
                        let tool_color = if self.tools_enabled {
                            egui::Color32::from_rgb(255, 200, 100)
                        } else {
                            ui.visuals().text_color()
                        };
                        let tr = ui.toggle_value(
                            &mut self.tools_enabled,
                            egui::RichText::new(format!("{} Tools", tool_icon)).color(tool_color),
                        );
                        let tool_tip = if self.tools_enabled {
                            if self.ai_workdir.trim().is_empty() {
                                "Tools actives MAIS pas de workdir IA defini ! Definissez-en un d'abord."
                            } else {
                                "Tools actives : list_dir, read_file, write_file, make_dir (jail dans le workdir IA)"
                            }
                        } else {
                            "Tools desactives. Le modele ne peut pas lire/ecrire de fichiers."
                        };
                        let tr = tr.on_hover_text(tool_tip);
                        if tr.changed() {
                            self.persist_settings();
                        }
                    },
                );
            });
        });

        // Barre de statut en bas — debug taille de la fenetre.
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let rect = ctx.screen_rect();
                let w = rect.width().round() as i32;
                let h = rect.height().round() as i32;
                let ppp = ctx.pixels_per_point();
                let px_w = (rect.width() * ppp).round() as i32;
                let px_h = (rect.height() * ppp).round() as i32;
                ui.weak(format!("🐛 debug"));
                ui.separator();
                ui.weak(format!("pts : {} x {}", w, h));
                ui.separator();
                ui.weak(format!("px : {} x {}", px_w, px_h));
                ui.separator();
                ui.weak(format!("ppp : {:.2}", ppp));
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.weak(format!(
                            "→ X : {}   Y : {}",
                            w, h
                        ));
                    },
                );
            });
            ui.add_space(2.0);
        });

        self.show_left_panel(ctx);
        self.show_workdir_confirm(ctx);
        if self.show_predictor {
            self.show_predictor_panel(ctx);
        }
        if self.show_thought_flow && !self.thought_flow.is_empty() {
            self.show_thought_flow_panel(ctx);
        }
        match self.view {
            View::Chat => self.show_chat(ctx),
            View::Persona => self.show_persona(ctx),
            View::Settings => self.show_settings(ctx),
        }
    }
}

impl App {
    fn show_thought_flow_panel(&mut self, ctx: &egui::Context) {
        // Trouve le dernier message assistant avec des tool_calls
        let last_tc_msg = self.messages.iter().rev()
            .find(|m| m.role == Role::Assistant && !m.tool_calls.is_empty());
        let Some(msg) = last_tc_msg else { return; };
        let tool_calls = msg.tool_calls.clone();
        let response_preview: String = msg.content.chars().take(60).collect();
        let response_full = msg.content.clone();
        let user_prompt: String = self.messages.iter().rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.chars().take(50).collect::<String>())
            .unwrap_or_default();
        let user_prompt_full: String = self.messages.iter().rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        egui::SidePanel::right("thought_flow")
            .resizable(true)
            .default_width(260.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.heading("🧠 Flow");
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui.small_button("✕").on_hover_text("Masquer").clicked() {
                                self.show_thought_flow = false;
                            }
                            if !self.ai_workdir.trim().is_empty() {
                                if ui.small_button("🌐").on_hover_text("Ouvrir dans le navigateur").clicked() {
                                    let html_path = Path::new(&self.ai_workdir).join("_thought_flow.html");
                                    let _ = std::process::Command::new("cmd")
                                        .args(["/C", "start", "", &html_path.display().to_string()])
                                        .spawn();
                                }
                            }
                            if ui.small_button("📋").on_hover_text("Copier un log texte des tool_calls (debug)").clicked() {
                                let mut s = String::new();
                                s.push_str(&format!("User: {}\n\n", user_prompt_full));
                                for (i, tc) in tool_calls.iter().enumerate() {
                                    let icon = if tc.is_error { "❌" } else { "✅" };
                                    s.push_str(&format!("[{}] {} {}\n", i + 1, tc.name, icon));
                                    s.push_str(&format!("  args: {}\n", tc.arguments));
                                    let result_preview: String = tc.result.chars().take(800).collect();
                                    let more = if tc.result.chars().count() > 800 { "\n  ... (tronque a 800 chars)" } else { "" };
                                    s.push_str(&format!("  result:\n{}{}\n\n", result_preview, more));
                                }
                                s.push_str(&format!("---\nResponse:\n{}\n", response_full));
                                ui.ctx().copy_text(s);
                            }
                        },
                    );
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let w = ui.available_width() - 16.0;
                        let box_h = 44.0f32;
                        let arrow_h = 20.0f32;
                        let corner = 6.0;

                        // --- Noeud User ---
                        let (rect, _) = ui.allocate_exact_size(
                            egui::Vec2::new(w, box_h),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, corner, egui::Color32::from_rgb(42, 74, 127));
                        ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.5, egui::Color32::from_rgb(85, 136, 204)), egui::StrokeKind::Outside);
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("User: {}", if user_prompt.len() > 40 { &user_prompt[..40] } else { &user_prompt }),
                            egui::FontId::proportional(12.0),
                            egui::Color32::WHITE,
                        );

                        // Fleche
                        let (arrow_rect, _) = ui.allocate_exact_size(
                            egui::Vec2::new(w, arrow_h),
                            egui::Sense::hover(),
                        );
                        let mid_x = arrow_rect.center().x;
                        ui.painter().line_segment(
                            [egui::Pos2::new(mid_x, arrow_rect.top()), egui::Pos2::new(mid_x, arrow_rect.bottom())],
                            egui::Stroke::new(2.0, egui::Color32::from_rgb(120, 120, 140)),
                        );
                        ui.painter().text(
                            egui::Pos2::new(mid_x + 8.0, arrow_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            "▼",
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_rgb(120, 120, 140),
                        );

                        // --- Noeuds Tools ---
                        for tc in &tool_calls {
                            let fill = if tc.is_error {
                                egui::Color32::from_rgb(127, 42, 42)
                            } else {
                                egui::Color32::from_rgb(90, 74, 26)
                            };
                            let stroke_color = if tc.is_error {
                                egui::Color32::from_rgb(204, 85, 85)
                            } else {
                                egui::Color32::from_rgb(204, 170, 68)
                            };
                            let status_icon = if tc.is_error { "X" } else { "OK" };

                            // Box tool
                            let (rect, _) = ui.allocate_exact_size(
                                egui::Vec2::new(w, box_h),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(rect, corner, fill);
                            ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.5, stroke_color), egui::StrokeKind::Outside);
                            // Nom du tool
                            ui.painter().text(
                                egui::Pos2::new(rect.left() + 8.0, rect.top() + 13.0),
                                egui::Align2::LEFT_CENTER,
                                format!("{} {}", tc.name, status_icon),
                                egui::FontId::proportional(12.0),
                                egui::Color32::WHITE,
                            );
                            // Args (petit, gris)
                            let args_short: String = tc.arguments.chars().take(35).collect();
                            ui.painter().text(
                                egui::Pos2::new(rect.left() + 8.0, rect.bottom() - 11.0),
                                egui::Align2::LEFT_CENTER,
                                &args_short,
                                egui::FontId::monospace(9.0),
                                egui::Color32::from_rgb(170, 170, 170),
                            );

                            // Fleche + apercu resultat
                            let result_short: String = tc.result.lines().next().unwrap_or("").chars().take(30).collect();
                            let (arrow_rect, _) = ui.allocate_exact_size(
                                egui::Vec2::new(w, arrow_h),
                                egui::Sense::hover(),
                            );
                            let mid_x = arrow_rect.center().x;
                            ui.painter().line_segment(
                                [egui::Pos2::new(mid_x, arrow_rect.top()), egui::Pos2::new(mid_x, arrow_rect.bottom())],
                                egui::Stroke::new(2.0, stroke_color),
                            );
                            if !result_short.is_empty() {
                                ui.painter().text(
                                    egui::Pos2::new(mid_x + 8.0, arrow_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &result_short,
                                    egui::FontId::proportional(9.0),
                                    egui::Color32::from_rgb(150, 160, 150),
                                );
                            }
                        }

                        // --- Noeud Reponse ---
                        let (rect, _) = ui.allocate_exact_size(
                            egui::Vec2::new(w, box_h),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, corner, egui::Color32::from_rgb(42, 111, 42));
                        ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.5, egui::Color32::from_rgb(85, 204, 85)), egui::StrokeKind::Outside);
                        let resp_text = if response_preview.is_empty() {
                            "(en cours...)".to_string()
                        } else if response_preview.len() > 50 {
                            format!("{}...", &response_preview[..50])
                        } else {
                            response_preview.clone()
                        };
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            resp_text,
                            egui::FontId::proportional(11.0),
                            egui::Color32::WHITE,
                        );

                        ui.add_space(12.0);
                        ui.weak(format!("{} tool calls", tool_calls.len()));
                    });
            });
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut added: Vec<&str> = Vec::new();

    let candidates: &[(&str, &str)] = &[
        ("cjk_meiryo", "C:\\Windows\\Fonts\\meiryo.ttc"),
        ("cjk_msgothic", "C:\\Windows\\Fonts\\msgothic.ttc"),
        ("cjk_yugothic", "C:\\Windows\\Fonts\\YuGothM.ttc"),
        ("emoji_color", "C:\\Windows\\Fonts\\seguiemj.ttf"),
        ("symbols", "C:\\Windows\\Fonts\\seguisym.ttf"),
        ("segoe", "C:\\Windows\\Fonts\\segoeui.ttf"),
    ];

    for (name, path) in candidates {
        if let Ok(data) = std::fs::read(path) {
            fonts
                .font_data
                .insert((*name).to_string(), egui::FontData::from_owned(data).into());
            added.push(name);
        }
    }

    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let entry = fonts.families.entry(fam).or_default();
        for name in &added {
            entry.push((*name).to_string());
        }
    }

    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 700.0])
            .with_resizable(true)
            .with_title("Mini Chat egui - LM Studio"),
        ..Default::default()
    };
    eframe::run_native(
        "Mini Chat egui",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(App::default()))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn list_dir_filters_thought_flow_artifacts() {
        let dir = std::env::temp_dir().join("test_egui_chat_filter_v9");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("_thought_flow.md"), "a").unwrap();
        fs::write(dir.join("_thought_flow.html"), "b").unwrap();
        fs::write(dir.join("normal.md"), "c").unwrap();
        fs::write(dir.join("_thought_flow_backup.md"), "d").unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();

        let entries = read_dir_limited(&dir, 200);
        let visible: Vec<&str> = entries
            .iter()
            .filter(|e| !e.name.starts_with("_thought_flow."))
            .map(|e| e.name.as_str())
            .collect();

        assert!(visible.contains(&"normal.md"));
        assert!(visible.contains(&"src"));
        assert!(visible.contains(&"_thought_flow_backup.md"));
        assert!(!visible.contains(&"_thought_flow.md"));
        assert!(!visible.contains(&"_thought_flow.html"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn task_db_schema_is_valid() {
        let path = std::env::temp_dir().join("test_task_state_v9.db");
        let _ = fs::remove_file(&path);
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_description TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                ended_at TEXT,
                model TEXT,
                final_summary TEXT
             );
             CREATE TABLE IF NOT EXISTS steps (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id INTEGER NOT NULL REFERENCES tasks(id),
                step_number INTEGER NOT NULL,
                description TEXT NOT NULL,
                status TEXT NOT NULL,
                findings TEXT,
                error TEXT,
                tool_calls_json TEXT,
                started_at TEXT,
                ended_at TEXT,
                tokens_used INTEGER
             );
             CREATE TABLE IF NOT EXISTS cycle_prompts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id INTEGER NOT NULL REFERENCES tasks(id),
                cycle_number INTEGER NOT NULL,
                system_prompt TEXT,
                user_prompt TEXT,
                response_text TEXT,
                ts TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_steps_task ON steps(task_id);
             CREATE INDEX IF NOT EXISTS idx_cycle_prompts_task ON cycle_prompts(task_id);",
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('tasks','steps','cycle_prompts')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);

        let idx_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('idx_steps_task','idx_cycle_prompts_task')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 2);

        drop(conn);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn knowledge_db_schema_is_valid() {
        let path = std::env::temp_dir().join("test_knowledge_v9.db");
        let _ = fs::remove_file(&path);
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS knowledge (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags TEXT,
                embedding BLOB NOT NULL,
                source TEXT,
                created_at TEXT,
                model TEXT
            )",
            [],
        )
        .unwrap();

        // Insert reel avec floats_to_blob pour valider le round-trip embedding.
        let emb = vec![0.1f32, 0.2, 0.3, 0.4];
        let blob = floats_to_blob(&emb);
        conn.execute(
            "INSERT INTO knowledge (title, content, tags, embedding, source, created_at, model)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params!["t", "c", "a,b", blob, "test", "2026-04-17", "nomic"],
        )
        .unwrap();

        let (title, content, got_blob): (String, String, Vec<u8>) = conn
            .query_row(
                "SELECT title, content, embedding FROM knowledge WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(title, "t");
        assert_eq!(content, "c");
        let round = blob_to_floats(&got_blob);
        assert_eq!(round.len(), 4);
        assert!((round[0] - 0.1).abs() < 1e-6);
        assert!((round[3] - 0.4).abs() < 1e-6);

        // Cosine similarity sanity : vecteur identique = 1.0
        let sim = cosine_similarity(&emb, &round);
        assert!((sim - 1.0).abs() < 1e-5);

        drop(conn);
        let _ = fs::remove_file(&path);
    }
}
