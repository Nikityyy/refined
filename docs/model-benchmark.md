# Groq study benchmark

I ran six synthetic study tasks on November 19, 2026. Each setting received the same six short passages, using Refined’s built-in prompts, automatic language, temperature 1, top P 1, and the maximum completion tokens returned by Groq. I rotated model order across tasks. Timings run from sending the request through receiving the response over HTTPS. They exclude the hotkey, clipboard, and UI.

| Groq model | Reasoning | Median | Range | Cases passed |
| --- | --- | ---: | ---: | ---: |
| ALLaM 2 7B | Model default | 201 ms | 189–378 ms | 3/6 |
| Qwen 3.8 27B | Off | 355 ms | 179–749 ms | 5/6 |
| GPT-OSS 20B | Low | 359 ms | 206–516 ms | 5/6 |
| GPT-OSS 120B | Low | 517 ms | 277–1,009 ms | 6/6 |
| GPT-OSS 20B | High | 1,256 ms | 801–2,949 ms | 6/6 |
| GPT-OSS 120B | High | 1,346 ms | 1,049–3,223 ms | 6/6 |

All 36 requests completed in this run. The pass count comes from checking the answers for factual fidelity, language, and the requested format. I allowed harmless differences in phrasing and typography. This is one run on short notes, so expect timings and results to vary.

- **Polish** had to keep the boiling point of water at 100 °C at 1 atm and preserve the relationship between pressure and boiling point.
- **Condense** had to retain the distinction between reaching equilibrium faster and changing the equilibrium constant.
- **Simplify** had to keep the explanation in German and preserve the direction of osmosis and the condition about opposing pressure.
- **Structure** had to keep the 1914, 1918, and 1919 events separate, with no added historical claims.
- **Flashcards** had to produce two Q/A pairs carrying the original derivative facts.
- **Expand** had to explain why doubling the net force doubles acceleration at a fixed mass, with a consistent example.

ALLaM was fastest. It answered the German task in English, added unsupported historical claims, and made an error in the force example. Qwen broadened the qualifier on opposing pressure. GPT-OSS 20B at Low answered the force example in German. The **Study preset** uses GPT-OSS 120B at Low, the fastest setting to pass every case. **Quick edits** uses GPT-OSS 20B at Low.

The key used for this test listed four chat models. Groq lists the Llama 3.1 and 3.3 chat models as enterprise-only. Refined excludes Prompt Guard from the picker because it is a classifier. [Groq models](https://console.groq.com/docs/models) · [Groq deprecations](https://console.groq.com/docs/deprecations)
