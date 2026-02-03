#!/usr/bin/env python3
"""
Thinking Model Detection Test

Detects if a model has reasoning/thinking capabilities by:
1. Sending prompts that trigger thinking mode
2. Checking for thinking patterns in output (<think>, reasoning tags, etc.)
3. Measuring thinking overhead (output much longer than expected)
"""

import requests
import json
import re
import sys
from datetime import datetime

OLLAMA_URL = "http://localhost:11434"

# Prompts designed to trigger thinking mode
THINKING_PROMPTS = [
    # Direct thinking trigger
    "Think step by step: What is 17 * 23?",

    # Complex reasoning problem
    "A farmer has 17 sheep. All but 9 run away. How many sheep does the farmer have left? Explain your reasoning.",

    # Logic puzzle
    "If all roses are flowers, and some flowers fade quickly, can we conclude that some roses fade quickly? Reason through this carefully.",
]

# Patterns that indicate thinking mode
THINKING_PATTERNS = [
    r'<think>',
    r'</think>',
    r'<reasoning>',
    r'</reasoning>',
    r'<thought>',
    r'</thought>',
    r'\*thinks?\*',
    r'\*reasoning\*',
    r'Let me think',
    r'Let me reason',
    r'\*?\*?Step \d+[:\*]',
    r'First,.*Second,.*Third,',
    r'I need to',
    r'I should',
    r'Breaking this down',
    r'To solve this',
]

def test_thinking_capability(model: str, timeout: int = 120) -> dict:
    """Test if a model has thinking/reasoning capabilities."""

    result = {
        "model": model,
        "timestamp": datetime.now().isoformat(),
        "is_thinking_model": False,
        "thinking_patterns_found": [],
        "thinking_tags_found": False,
        "reasoning_verbose": False,
        "tests": []
    }

    print(f"\n{'='*60}")
    print(f"Testing: {model}")
    print(f"{'='*60}")

    for i, prompt in enumerate(THINKING_PROMPTS):
        print(f"\n  Test {i+1}: {prompt[:50]}...")

        try:
            # First try raw mode to catch hidden thinking tags
            resp = requests.post(f'{OLLAMA_URL}/api/generate', json={
                'model': model,
                'prompt': prompt,
                'stream': False,
                'raw': True,  # Raw mode to preserve thinking tags
                'options': {
                    'num_ctx': 8192,
                    'num_predict': 2048,  # Allow room for thinking
                    'temperature': 0.7
                }
            }, timeout=timeout)

            if resp.status_code == 200:
                data = resp.json()
                response = data.get('response', '')
                output_tokens = data.get('eval_count', 0)

                test_record = {
                    "prompt": prompt[:50],
                    "output_tokens": output_tokens,
                    "patterns_found": []
                }

                # Check for thinking patterns
                for pattern in THINKING_PATTERNS:
                    if re.search(pattern, response, re.IGNORECASE):
                        test_record["patterns_found"].append(pattern)
                        if pattern not in result["thinking_patterns_found"]:
                            result["thinking_patterns_found"].append(pattern)

                # Check for explicit thinking tags
                if re.search(r'<think>|</think>|<reasoning>|</reasoning>', response, re.IGNORECASE):
                    result["thinking_tags_found"] = True
                    test_record["has_thinking_tags"] = True

                # Check if response is verbose (thinking models often produce longer output)
                if output_tokens > 200:
                    result["reasoning_verbose"] = True

                test_record["response_preview"] = response[:300] + "..." if len(response) > 300 else response
                result["tests"].append(test_record)

                patterns_count = len(test_record["patterns_found"])
                print(f"    Output: {output_tokens} tokens, {patterns_count} thinking patterns")

                if test_record.get("has_thinking_tags"):
                    print(f"    THINKING TAGS DETECTED!")

            else:
                print(f"    FAILED: HTTP {resp.status_code}")
                result["tests"].append({"prompt": prompt[:50], "error": f"HTTP {resp.status_code}"})

        except Exception as e:
            print(f"    ERROR: {e}")
            result["tests"].append({"prompt": prompt[:50], "error": str(e)[:50]})

    # Determine if this is a thinking model
    if result["thinking_tags_found"]:
        result["is_thinking_model"] = True
        result["thinking_type"] = "explicit_tags"
    elif len(result["thinking_patterns_found"]) >= 3:
        result["is_thinking_model"] = True
        result["thinking_type"] = "pattern_based"
    elif result["reasoning_verbose"] and len(result["thinking_patterns_found"]) >= 2:
        result["is_thinking_model"] = True
        result["thinking_type"] = "verbose_reasoning"
    else:
        result["thinking_type"] = "none"

    print(f"\n  Result: {'THINKING MODEL' if result['is_thinking_model'] else 'Standard model'}")
    print(f"  Type: {result['thinking_type']}")
    print(f"  Patterns: {len(result['thinking_patterns_found'])}")

    return result

def main():
    # Models to test - focus on suspected thinking models first
    suspected_thinking = [
        "deepseek-r1:14b",
        "hf.co/DevQuasar/FreedomIntelligence.HuatuoGPT-o1-8B-GGUF:Q4_K_M",
        "hf.co/mradermacher/Mistral-Nemo-Inst-2407-12B-Thinking-Uncensored-HERETIC-HI-Claude-Opus-i1-GGUF:Q4_K_M",
        "cogito:8b",
        "hf.co/DavidAU/Qwen3-24B-A4B-Freedom-HQ-Thinking-Abliterated-Heretic-NEOMAX-Imatrix-GGUF:latest",
        "phi3:mini",
        "exaone-deep:7.8b",
    ]

    # Also test some standard models as baseline
    standard_models = [
        "llama3.1:8b",
        "qwen2.5:7b",
        "mistral:latest",
        "gemma2:9b",
    ]

    # Use command line args or defaults
    if len(sys.argv) > 1:
        models_to_test = sys.argv[1:]
    else:
        models_to_test = suspected_thinking + standard_models

    print("="*60)
    print("THINKING MODEL DETECTION TEST")
    print(f"Started: {datetime.now().isoformat()}")
    print(f"Models to test: {len(models_to_test)}")
    print("="*60)

    all_results = []

    for model in models_to_test:
        try:
            result = test_thinking_capability(model)
            all_results.append(result)
        except Exception as e:
            print(f"Error testing {model}: {e}")
            all_results.append({
                "model": model,
                "error": str(e),
                "timestamp": datetime.now().isoformat()
            })

    # Summary
    print("\n" + "="*60)
    print("SUMMARY")
    print("="*60)

    thinking_models = [r for r in all_results if r.get("is_thinking_model")]
    standard = [r for r in all_results if not r.get("is_thinking_model") and not r.get("error")]

    print(f"\nThinking Models ({len(thinking_models)}):")
    for r in thinking_models:
        print(f"  - {r['model']}: {r.get('thinking_type', 'unknown')}")

    print(f"\nStandard Models ({len(standard)}):")
    for r in standard:
        print(f"  - {r['model']}")

    # Save results
    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
    output_file = f"/home/roctinam/dev/fortemi/docs/research/thinking_test_results_{timestamp}.json"
    with open(output_file, 'w') as f:
        json.dump(all_results, f, indent=2)
    print(f"\nResults saved to: {output_file}")

    return all_results

if __name__ == "__main__":
    main()
