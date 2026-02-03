#!/usr/bin/env python3
"""
Batch Context Length Testing - Tests multiple models efficiently.
Finds the breaking point for each model.
"""

import requests
import time
import json
import sys
from datetime import datetime

OLLAMA_URL = "http://localhost:11434"

def test_context(model: str, num_ctx: int, timeout: int = 300) -> tuple:
    """Test a specific context size. Returns (success, tokens_processed, time_seconds)."""
    prompt = "test " * (num_ctx // 2)
    try:
        start = time.time()
        resp = requests.post(f'{OLLAMA_URL}/api/generate', json={
            'model': model,
            'prompt': f'Summarize: {prompt}',
            'stream': False,
            'options': {'num_ctx': num_ctx, 'num_predict': 10}
        }, timeout=timeout)
        elapsed = time.time() - start

        if resp.status_code == 200:
            data = resp.json()
            return True, data.get('prompt_eval_count', 0), elapsed
        return False, f"HTTP {resp.status_code}", 0
    except Exception as e:
        return False, str(e)[:50], 0

def find_max_context(model: str) -> dict:
    """Binary search to find max context for a model."""
    print(f"\n{'='*60}")
    print(f"Testing: {model}")
    print(f"{'='*60}")

    result = {
        "model": model,
        "timestamp": datetime.now().isoformat(),
        "tests": []
    }

    # Test increasing context sizes until failure
    context_sizes = [4096, 8192, 16384, 32768, 65536, 131072, 196608, 262144, 524288]

    max_successful = 0
    max_tokens = 0

    for ctx in context_sizes:
        print(f"  Testing {ctx//1024}K...", end=" ", flush=True)
        ok, tokens_or_error, elapsed = test_context(model, ctx)

        test_record = {
            "num_ctx": ctx,
            "success": ok
        }

        if ok:
            print(f"OK ({tokens_or_error} tokens, {elapsed:.1f}s)")
            max_successful = ctx
            max_tokens = tokens_or_error
            test_record["tokens"] = tokens_or_error
            test_record["time_s"] = round(elapsed, 2)
        else:
            print(f"FAILED - {tokens_or_error}")
            test_record["error"] = tokens_or_error
            result["tests"].append(test_record)
            break

        result["tests"].append(test_record)

    result["max_num_ctx"] = max_successful
    result["max_tokens"] = max_tokens
    result["recommended_ctx"] = int(max_successful * 0.85)  # 85% safety margin

    print(f"\n  Max num_ctx: {max_successful} ({max_successful//1024}K)")
    print(f"  Max tokens:  {max_tokens}")
    print(f"  Recommended: {result['recommended_ctx']} ({result['recommended_ctx']//1024}K)")

    return result

def main():
    models = sys.argv[1:] if len(sys.argv) > 1 else [
        "qwen2.5:14b",
        "qwen2.5-coder:14b",
        "gemma2:9b",
        "phi3:mini",
        "codestral:latest",
        "llama3.1:8b"
    ]

    print("="*60)
    print("BATCH CONTEXT LENGTH TESTING")
    print(f"Started: {datetime.now().isoformat()}")
    print(f"Models to test: {len(models)}")
    print("="*60)

    all_results = []

    for model in models:
        try:
            result = find_max_context(model)
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
    print(f"\n{'Model':<30} {'Max CTX':>10} {'Max Tokens':>12} {'Recommended':>12}")
    print("-"*66)

    for r in all_results:
        if "error" in r:
            print(f"{r['model']:<30} ERROR: {r['error']}")
        else:
            print(f"{r['model']:<30} {r.get('max_num_ctx', 0)//1024:>7}K {r.get('max_tokens', 0):>12} {r.get('recommended_ctx', 0)//1024:>9}K")

    # Save results
    output_file = f"/home/roctinam/dev/fortemi/docs/research/batch_context_results_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
    with open(output_file, 'w') as f:
        json.dump(all_results, f, indent=2)
    print(f"\nResults saved to: {output_file}")

    return all_results

if __name__ == "__main__":
    main()
