#!/usr/bin/env python3
"""
Generate multilingual text samples for UAT testing.

Creates text files in various languages to test:
- FTS stemming (English, German, French, Spanish, Portuguese, Russian)
- CJK bigram matching (Chinese, Japanese, Korean)
- Basic tokenization (Arabic, Greek, Hebrew)
- Emoji/trigram search
"""

from pathlib import Path


SAMPLES = {
    "english.txt": """The quick brown fox jumps over the lazy dog. This sentence contains every letter of the English alphabet at least once.

Natural language processing enables computers to understand, interpret, and generate human language. Modern NLP systems use transformer architectures and attention mechanisms to achieve state-of-the-art results on tasks like translation, summarization, and question answering.

Full-text search with stemming allows users to find documents even when they search for different word forms. For example, searching for "run" should also match "running", "runs", and "ran". PostgreSQL's websearch_to_tsquery function handles this automatically for English text.

Testing edge cases is crucial for robust software. Consider boundary values, empty inputs, null pointers, and unicode characters. Comprehensive test coverage catches bugs early in the development cycle.

Machine learning models require large amounts of training data. The quality of the data directly impacts model performance. Data preprocessing steps include normalization, tokenization, and feature extraction. Modern embeddings like BERT and GPT transform text into dense vector representations.
""",

    "german.txt": """Die deutsche Sprache gehört zur westgermanischen Sprachgruppe und wird von über 100 Millionen Menschen gesprochen.

Volltext-Suche mit Wortstammerkennung ermöglicht es Benutzern, Dokumente zu finden, auch wenn sie nach verschiedenen Wortformen suchen. Zum Beispiel sollte die Suche nach "laufen" auch "läuft", "lief" und "gelaufen" finden. PostgreSQL unterstützt deutsche Wortstammerkennung durch die entsprechende Sprachkonfiguration.

Natürliche Sprachverarbeitung (NLP) hat in den letzten Jahren enorme Fortschritte gemacht. Moderne Systeme können Texte übersetzen, zusammenfassen und Fragen beantworten. Die Transformer-Architektur hat dabei eine Schlüsselrolle gespielt.

Umlaute wie ä, ö und ü sind wichtige Bestandteile der deutschen Schrift. Das ß (Eszett) wird in Deutschland verwendet, während in der Schweiz ss geschrieben wird.

Künstliche Intelligenz verändert viele Bereiche unseres Lebens. Maschinelles Lernen erfordert große Mengen an Trainingsdaten. Die Datenqualität beeinflusst die Modellleistung direkt.
""",

    "french.txt": """Le français est une langue romane parlée par environ 300 millions de personnes dans le monde.

La recherche en texte intégral avec normalisation permet aux utilisateurs de trouver des documents même lorsqu'ils recherchent différentes formes de mots. Par exemple, la recherche de "courir" devrait également correspondre à "cours", "courons" et "couru". PostgreSQL prend en charge la normalisation française via sa configuration linguistique.

Le traitement du langage naturel (NLP) a connu des progrès remarquables ces dernières années. Les systèmes modernes peuvent traduire, résumer et répondre aux questions. L'architecture Transformer a joué un rôle clé dans ces avancées.

Les accents français incluent l'aigu (é), le grave (è), le circonflexe (ê) et la cédille (ç). Ces signes diacritiques sont essentiels pour la prononciation et le sens correct.

L'intelligence artificielle transforme de nombreux aspects de nos vies. L'apprentissage automatique nécessite de grandes quantités de données d'entraînement. La qualité des données impacte directement les performances du modèle.
""",

    "spanish.txt": """El español es una lengua romance hablada por más de 500 millones de personas en todo el mundo.

La búsqueda de texto completo con lematización permite a los usuarios encontrar documentos incluso cuando buscan diferentes formas de palabras. Por ejemplo, buscar "correr" también debería encontrar "corre", "corriendo" y "corrió". PostgreSQL admite la lematización española a través de su configuración de idioma.

El procesamiento del lenguaje natural (PLN) ha experimentado avances notables en los últimos años. Los sistemas modernos pueden traducir, resumir y responder preguntas. La arquitectura Transformer ha desempeñado un papel clave en estos avances.

Los acentos españoles incluyen la tilde (á, é, í, ó, ú) y la diéresis (ü). La letra ñ es característica única del español. Los signos de interrogación (¿?) y exclamación (¡!) se usan al principio y al final de las oraciones.

La inteligencia artificial está transformando muchos aspectos de nuestras vidas. El aprendizaje automático requiere grandes cantidades de datos de entrenamiento.
""",

    "portuguese.txt": """O português é uma língua românica falada por mais de 250 milhões de pessoas em todo o mundo.

A pesquisa de texto completo com lematização permite que os usuários encontrem documentos mesmo quando pesquisam diferentes formas de palavras. Por exemplo, pesquisar "correr" também deve encontrar "corre", "correndo" e "correu". PostgreSQL suporta lematização portuguesa através de sua configuração de idioma.

O processamento de linguagem natural (PLN) experimentou avanços notáveis nos últimos anos. Sistemas modernos podem traduzir, resumir e responder perguntas. A arquitetura Transformer desempenhou um papel fundamental nesses avanços.

Os acentos portugueses incluem agudo (á, é), circunflexo (â, ê, ô), til (ã, õ) e crase (à). A cedilha (ç) também é usada. Existem diferenças entre o português europeu e o brasileiro.

A inteligência artificial está transformando muitos aspectos de nossas vidas. O aprendizado de máquina requer grandes quantidades de dados de treinamento.
""",

    "russian.txt": """Русский язык является восточнославянским языком и используется более чем 250 миллионами человек по всему миру.

Полнотекстовый поиск с основами слов позволяет пользователям находить документы, даже если они ищут разные формы слов. Например, поиск "бежать" должен также находить "бежит", "бегут" и "бежал". PostgreSQL поддерживает русское словообразование через соответствующую языковую конфигурацию.

Обработка естественного языка (NLP) достигла замечательных успехов в последние годы. Современные системы могут переводить, резюмировать и отвечать на вопросы. Архитектура трансформера сыграла ключевую роль в этих достижениях.

Кириллица используется для написания русского языка. Буквы включают а, б, в, г, д, е, ё, ж, з, и, й, к, л, м, н, о, п, р, с, т, у, ф, х, ц, ч, ш, щ, ъ, ы, ь, э, ю, я.

Искусственный интеллект меняет многие аспекты нашей жизни. Машинное обучение требует больших объемов обучающих данных.
""",

    "chinese-simplified.txt": """中文是世界上使用人数最多的语言之一，有超过十亿人使用。

全文搜索对于中日韩(CJK)语言使用字符二元组匹配，因为这些语言不使用空格分隔单词。PostgreSQL通过pg_bigm扩展支持CJK文本的高效搜索。

自然语言处理(NLP)技术在近年来取得了显著进展。现代系统可以翻译、摘要和回答问题。Transformer架构在这些进展中发挥了关键作用。

中文文本包含常用汉字、标点符号和阿拉伯数字。简体中文在中国大陆使用，而繁体中文在台湾和香港使用。搜索"北京"应该能找到包含"北京市"、"北京大学"的文档。

人工智能正在改变我们生活的许多方面。机器学习需要大量的训练数据。数据质量直接影响模型性能。

语义搜索使用向量嵌入来理解查询意图。现代嵌入模型可以捕捉单词和句子的语义含义。
""",

    "japanese.txt": """日本語は日本で話されている言語で、約1億2500万人が使用しています。

全文検索はCJK言語に対してバイグラム(2文字組み合わせ)マッチングを使用します。これらの言語は単語を空白で区切らないため、PostgreSQLのpg_bigm拡張機能を使用して効率的な検索を実現します。

自然言語処理(NLP)技術は近年著しい進歩を遂げています。最新のシステムは翻訳、要約、質問応答が可能です。Transformerアーキテクチャがこれらの進歩において重要な役割を果たしました。

日本語のテキストには、ひらがな、カタカナ、漢字が含まれます。「東京」を検索すると「東京都」や「東京大学」を含む文書が見つかるはずです。

人工知能は私たちの生活の多くの側面を変えています。機械学習には大量のトレーニングデータが必要です。

セマンティック検索はベクトル埋め込みを使用してクエリの意図を理解します。現代の埋め込みモデルは単語や文の意味を捉えることができます。
""",

    "korean.txt": """한국어는 한국과 북한에서 사용되는 언어로 약 7700만 명이 사용합니다.

전체 텍스트 검색은 CJK 언어에 대해 바이그램(2글자 조합) 매칭을 사용합니다. 이러한 언어는 공백으로 단어를 구분하지 않기 때문에 PostgreSQL의 pg_bigm 확장을 사용하여 효율적인 검색을 구현합니다.

자연어 처리(NLP) 기술은 최근 몇 년간 현저한 발전을 이루었습니다. 최신 시스템은 번역, 요약, 질문 응답이 가능합니다. Transformer 아키텍처가 이러한 발전에 핵심적인 역할을 했습니다.

한국어 텍스트는 한글로 구성됩니다. "서울"을 검색하면 "서울시"나 "서울대학교"가 포함된 문서를 찾을 수 있어야 합니다.

인공지능은 우리 생활의 많은 측면을 변화시키고 있습니다. 머신러닝은 대량의 훈련 데이터가 필요합니다.

의미론적 검색은 벡터 임베딩을 사용하여 쿼리 의도를 이해합니다.
""",

    "arabic.txt": """اللغة العربية هي إحدى أكثر اللغات انتشارًا في العالم، حيث يتحدث بها أكثر من 400 مليون شخص.

يستخدم البحث النصي الكامل للغات التي تُكتب من اليمين إلى اليسار مثل العربية الترميز الصحيح. يدعم PostgreSQL النصوص العربية من خلال تكوين اللغة المناسب.

شهدت معالجة اللغة الطبيعية تقدمًا ملحوظًا في السنوات الأخيرة. يمكن للأنظمة الحديثة الترجمة والتلخيص والإجابة على الأسئلة. لعبت بنية المحول دورًا رئيسيًا في هذه التطورات.

النص العربي يتضمن علامات التشكيل مثل الفتحة والكسرة والضمة. اللغة العربية تُكتب من اليمين إلى اليسار وتحتوي على 28 حرفًا.

الذكاء الاصطناعي يغير العديد من جوانب حياتنا. يتطلب التعلم الآلي كميات كبيرة من بيانات التدريب.

البحث الدلالي يستخدم التضمينات المتجهة لفهم نية الاستعلام.
""",

    "greek.txt": """Η ελληνική γλώσσα είναι μία από τις αρχαιότερες γλώσσες στον κόσμο και ομιλείται από περίπου 13 εκατομμύρια ανθρώπους.

Η αναζήτηση πλήρους κειμένου για την ελληνική χρησιμοποιεί βασική τμηματοποίηση. Το PostgreSQL υποστηρίζει ελληνικό κείμενο μέσω της κατάλληλης γλωσσικής διαμόρφωσης.

Η επεξεργασία φυσικής γλώσσας έχει σημειώσει αξιοσημείωτη πρόοδο τα τελευταία χρόνια. Τα σύγχρονα συστήματα μπορούν να μεταφράζουν, να συνοψίζουν και να απαντούν σε ερωτήσεις.

Το ελληνικό αλφάβητο περιλαμβάνει γράμματα όπως α, β, γ, δ, ε, ζ, η, θ, ι, κ, λ, μ, ν, ξ, ο, π, ρ, σ, τ, υ, φ, χ, ψ, ω.

Η τεχνητή νοημοσύνη αλλάζει πολλές πτυχές της ζωής μας. Η μηχανική μάθηση απαιτεί μεγάλες ποσότητες δεδομένων εκπαίδευσης.
""",

    "hebrew.txt": """העברית היא שפה שמית המדוברת על ידי כ-9 מיליון אנשים ברחבי העולם.

חיפוש טקסט מלא לשפות הנכתבות מימין לשמאל כמו עברית משתמש בקידוד נכון. PostgreSQL תומך בטקסט עברי באמצעות תצורת השפה המתאימה.

עיבוד שפה טבעית חווה התקדמות ניכרת בשנים האחרונות. מערכות מודרניות יכולות לתרגם, לסכם ולענות על שאלות. ארכיטקטורת הטרנספורמר שיחקה תפקיד מרכזי בהתקדמות זו.

הטקסט העברי כולל ניקוד אך בדרך כלל נכתב בלי אותו. האלפבית העברי מכיל 22 אותיות.

בינה מלאכותית משנה היבטים רבים בחיינו. למידת מכונה דורשת כמויות גדולות של נתוני אימון.

חיפוש סמנטי משתמש בהטמעות וקטוריות כדי להבין את כוונת השאילתה.
""",

    "emoji-heavy.txt": """🎉 Welcome to Matric Memory! 🚀

Full-text search supports emoji through trigram indexing. 🔍✨

Common emoji usage:
- 😀😁😂🤣 Happy faces
- 🔥💯👍 Positive reactions
- 🌟⭐✨ Stars and sparkles
- 🎯🎨🎭 Activities
- 🌍🌎🌏 World globes
- 💻📱⌨️ Technology
- 🍕🍔🍟 Food
- 🚀🛸✈️ Transportation
- ❤️💙💚 Hearts and colors
- 🎵🎶🎸 Music
- 🏃‍♂️🏊‍♀️⚽ Sports
- 🌈☀️⛈️ Weather

Emoji can be searched individually: 🎉 or combined: 🚀🌟

PostgreSQL's pg_trgm extension enables substring matching for emoji characters, allowing users to search for "🎉" and find all documents containing that specific emoji. 🎊🎈

Testing various emoji categories: 🐶🐱🐭🐹🐰🦊🐻🐼🐨🐯🦁🐮🐷🐸🐵

Numbers and symbols work too: 0️⃣1️⃣2️⃣3️⃣4️⃣5️⃣6️⃣7️⃣8️⃣9️⃣🔟

Special characters: ™️©️®️💯🔞⚠️🚫✅❌

Emoji with skin tones: 👋👋🏻👋🏼👋🏽👋🏾👋🏿

Combined emoji: 👨‍👩‍👧‍👦 👨‍💻 🧑‍🚀 👩‍🔬

Flag emoji: 🇺🇸🇬🇧🇫🇷🇩🇪🇯🇵🇨🇳🇰🇷🇧🇷

Emoji reactions for testing: 👍👎👌✌️🤞🤟🤘🤙👏🙌
""",
}


def main():
    script_dir = Path(__file__).parent
    data_dir = script_dir.parent
    multilingual_dir = data_dir / "multilingual"
    multilingual_dir.mkdir(parents=True, exist_ok=True)

    print("Generating multilingual text samples...")

    for filename, content in SAMPLES.items():
        filepath = multilingual_dir / filename
        filepath.write_text(content, encoding='utf-8')
        print(f"  ✓ Created {filename}")

    print("")
    print(f"✓ Generated {len(SAMPLES)} multilingual text files")
    print("")
    print("Language coverage:")
    print("  FTS Stemming: English, German, French, Spanish, Portuguese, Russian")
    print("  CJK Bigram: Chinese, Japanese, Korean")
    print("  Basic Tokenization: Arabic, Greek, Hebrew")
    print("  Trigram: Emoji")


if __name__ == "__main__":
    main()
