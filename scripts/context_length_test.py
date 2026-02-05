#!/usr/bin/env python3
"""
Context Length Testing Script for Ollama Models

Tests practical context length limits and performance characteristics
of models installed on the local Ollama instance.

Key enhancement: Tests with variable num_ctx parameter to find true
maximum context capacity, not just default limits.
"""

import requests
import time
import json
import sys
from datetime import datetime
from typing import Optional

OLLAMA_URL = "http://localhost:11434"

def get_models():
    """Get list of installed models."""
    resp = requests.get(f"{OLLAMA_URL}/api/tags")
    return [m["name"] for m in resp.json().get("models", [])]

def count_tokens_approx(text):
    """Approximate token count (chars / 4)."""
    return len(text) // 4

def generate_test_prompt(target_tokens):
    """Generate a prompt with approximately target_tokens tokens."""
    base = "The quick brown fox jumps over the lazy dog. "  # ~12 tokens
    repeats = max(1, target_tokens // 12)
    padding = base * repeats

    prompt = f"""Please summarize the following text in 2-3 sentences:

{padding}

Summary:"""
    return prompt

def test_single_context(model_name: str, target_tokens: int, num_ctx: Optional[int] = None, timeout: int = 180):
    """Test a single context size with optional num_ctx override."""
    prompt = generate_test_prompt(target_tokens)
    actual_tokens = count_tokens_approx(prompt)

    options = {"num_predict": 100}
    if num_ctx is not None:
        options["num_ctx"] = num_ctx

    try:
        start = time.time()
        resp = requests.post(
            f"{OLLAMA_URL}/api/generate",
            json={
                "model": model_name,
                "prompt": prompt,
                "stream": False,
                "options": options
            },
            timeout=timeout
        )
        elapsed = time.time() - start

        if resp.status_code == 200:
            data = resp.json()
            prompt_eval = data.get("prompt_eval_count", 0)

            # Check if truncation occurred
            truncated = prompt_eval < actual_tokens * 0.9  # Allow 10% variance

            return {
                "success": True,
                "target_tokens": target_tokens,
                "actual_tokens_approx": actual_tokens,
                "num_ctx": num_ctx,
                "prompt_eval_count": prompt_eval,
                "eval_count": data.get("eval_count", 0),
                "response_time_s": round(elapsed, 2),
                "truncated": truncated,
                "utilization": round(prompt_eval / actual_tokens * 100, 1) if actual_tokens > 0 else 0
            }
        else:
            return {
                "success": False,
                "target_tokens": target_tokens,
                "num_ctx": num_ctx,
                "error": f"HTTP {resp.status_code}: {resp.text[:200]}"
            }
    except requests.Timeout:
        return {
            "success": False,
            "target_tokens": target_tokens,
            "num_ctx": num_ctx,
            "error": f"Timeout after {timeout}s"
        }
    except Exception as e:
        return {
            "success": False,
            "target_tokens": target_tokens,
            "num_ctx": num_ctx,
            "error": str(e)
        }

def find_max_context(model_name: str, timeout: int = 180):
    """
    Find the maximum practical context for a model.

    Strategy:
    1. Test with default settings to find default limit
    2. Test with increasing num_ctx to find true maximum
    3. Binary search to find precise limit
    """
    results = {
        "model": model_name,
        "timestamp": datetime.now().isoformat(),
        "default_context": None,
        "max_context_with_num_ctx": None,
        "recommended_context": None,
        "tests": []
    }

    print(f"\n{'='*60}")
    print(f"Testing: {model_name}")
    print(f"{'='*60}")

    # Phase 1: Find default context limit
    print("\n[Phase 1] Testing default context limit...")
    test_sizes = [2000, 4000, 8000, 16000]

    default_max = 0
    for size in test_sizes:
        result = test_single_context(model_name, size, num_ctx=None, timeout=timeout)
        results["tests"].append(result)

        if result["success"]:
            if not result.get("truncated", False):
                default_max = result["prompt_eval_count"]
                print(f"  {size} tokens: OK (eval: {result['prompt_eval_count']}, {result['response_time_s']}s)")
            else:
                print(f"  {size} tokens: TRUNCATED at {result['prompt_eval_count']} (default limit reached)")
                break
        else:
            print(f"  {size} tokens: FAILED - {result.get('error', 'unknown')}")
            break

    results["default_context"] = default_max
    print(f"  Default context limit: ~{default_max} tokens")

    # Phase 2: Test with extended num_ctx
    print("\n[Phase 2] Testing extended context with num_ctx parameter...")
    num_ctx_values = [8192, 16384, 32768, 65536, 131072]

    max_with_numctx = default_max
    best_numctx = None

    for num_ctx in num_ctx_values:
        # Use test prompt size matching num_ctx
        test_size = int(num_ctx * 0.9)  # 90% of context for testing
        result = test_single_context(model_name, test_size, num_ctx=num_ctx, timeout=timeout)
        results["tests"].append(result)

        if result["success"]:
            prompt_eval = result["prompt_eval_count"]
            if prompt_eval > max_with_numctx:
                max_with_numctx = prompt_eval
                best_numctx = num_ctx
                print(f"  num_ctx={num_ctx}: eval={prompt_eval} tokens ({result['response_time_s']}s)")
            else:
                print(f"  num_ctx={num_ctx}: eval={prompt_eval} (no improvement, likely at max)")
                # If we're not seeing improvements, we've hit the model/hardware limit
                if prompt_eval <= max_with_numctx * 1.1:  # Less than 10% improvement
                    break
        else:
            print(f"  num_ctx={num_ctx}: FAILED - {result.get('error', 'unknown')}")
            # This context size is too large for the hardware
            break

    results["max_context_with_num_ctx"] = max_with_numctx
    results["optimal_num_ctx"] = best_numctx

    # Recommended = 80% of max for safety margin
    results["recommended_context"] = int(max_with_numctx * 0.8)

    print(f"\n  Maximum context: ~{max_with_numctx} tokens")
    print(f"  Optimal num_ctx: {best_numctx}")
    print(f"  Recommended limit: ~{results['recommended_context']} tokens")

    return results

def test_model_comprehensive(model_name: str, timeout: int = 180):
    """Comprehensive test including quality checks."""
    results = find_max_context(model_name, timeout)

    # Phase 3: Performance at different context sizes
    print("\n[Phase 3] Performance characteristics...")
    perf_tests = []

    test_contexts = [2000, 4000, 8000]
    if results["max_context_with_num_ctx"] > 16000:
        test_contexts.append(16000)
    if results["max_context_with_num_ctx"] > 32000:
        test_contexts.append(32000)

    optimal_ctx = results.get("optimal_num_ctx") or results["max_context_with_num_ctx"]

    for ctx in test_contexts:
        result = test_single_context(model_name, ctx, num_ctx=optimal_ctx, timeout=timeout)
        if result["success"]:
            perf_tests.append({
                "context_size": ctx,
                "time_s": result["response_time_s"],
                "tokens_per_sec": round(result["prompt_eval_count"] / result["response_time_s"], 1) if result["response_time_s"] > 0 else 0
            })
            print(f"  {ctx} tokens: {result['response_time_s']}s ({perf_tests[-1]['tokens_per_sec']} tok/s)")

    results["performance"] = perf_tests

    return results

def main():
    print("="*60)
    print("OLLAMA MODEL CONTEXT LENGTH TESTING")
    print("Enhanced with num_ctx parameter testing")
    print("="*60)
    print(f"Started: {datetime.now().isoformat()}")
    print(f"Ollama URL: {OLLAMA_URL}")

    # Parse arguments
    models_to_test = []
    comprehensive = "--comprehensive" in sys.argv or "-c" in sys.argv

    for arg in sys.argv[1:]:
        if not arg.startswith("-"):
            models_to_test.append(arg)

    if not models_to_test:
        models_to_test = ["gpt-oss:20b"]  # Default

    all_results = []

    for model in models_to_test:
        try:
            if comprehensive:
                results = test_model_comprehensive(model)
            else:
                results = find_max_context(model)
            all_results.append(results)
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

    for r in all_results:
        print(f"\n{r['model']}:")
        if "error" in r and r["error"]:
            print(f"  ERROR: {r['error']}")
            continue

        print(f"  Default context:     ~{r.get('default_context', 'N/A')} tokens")
        print(f"  Max context:         ~{r.get('max_context_with_num_ctx', 'N/A')} tokens")
        print(f"  Optimal num_ctx:     {r.get('optimal_num_ctx', 'N/A')}")
        print(f"  Recommended limit:   ~{r.get('recommended_context', 'N/A')} tokens")

        if r.get("performance"):
            avg_tps = sum(p["tokens_per_sec"] for p in r["performance"]) / len(r["performance"])
            print(f"  Avg throughput:      {avg_tps:.1f} tok/s")

    # Save results
    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
    output_file = f"/home/roctinam/dev/fortemi/docs/research/context_length_results_{timestamp}.json"
    with open(output_file, 'w') as f:
        json.dump(all_results, f, indent=2)
    print(f"\nResults saved to: {output_file}")

    return all_results

if __name__ == "__main__":
    main()
