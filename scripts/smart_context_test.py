#!/usr/bin/env python3
"""
Smart Context Length Testing - Efficiently finds native context limits.
Stops testing once truncation is detected or hardware limit is reached.
"""

import requests
import time
import json
import sys
from datetime import datetime

OLLAMA_URL = "http://localhost:11434"

def get_model_info(model: str) -> dict:
    """Get model details from Ollama."""
    try:
        resp = requests.post(f'{OLLAMA_URL}/api/show', json={'name': model}, timeout=30)
        if resp.status_code == 200:
            data = resp.json()
            # Try to find context_length in parameters
            params = data.get('parameters', '')
            details = data.get('details', {})
            return {
                'parameters': params,
                'family': details.get('family', 'unknown'),
                'parameter_size': details.get('parameter_size', 'unknown'),
                'context_length': details.get('context_length')  # May be None
            }
    except:
        pass
    return {}

def test_context(model: str, num_ctx: int, timeout: int = 300) -> dict:
    """Test context with given num_ctx. Returns actual tokens processed."""
    # Generate prompt slightly larger than num_ctx to detect truncation
    prompt = "test " * (num_ctx // 2)
    input_approx = len(prompt) // 4  # Rough token estimate

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
            tokens = data.get('prompt_eval_count', 0)
            return {
                'success': True,
                'input_approx': input_approx,
                'tokens_processed': tokens,
                'truncated': tokens < input_approx * 0.8,  # >20% loss = truncation
                'time_s': round(elapsed, 2)
            }
        return {'success': False, 'error': f"HTTP {resp.status_code}"}
    except Exception as e:
        return {'success': False, 'error': str(e)[:50]}

def find_native_limit(model: str) -> dict:
    """Find the model's native context limit efficiently."""
    print(f"\n{'='*60}")
    print(f"Testing: {model}")
    print(f"{'='*60}")

    # Get declared info
    info = get_model_info(model)
    print(f"  Family: {info.get('family', 'unknown')}")
    print(f"  Size: {info.get('parameter_size', 'unknown')}")
    if info.get('context_length'):
        print(f"  Declared context: {info['context_length']}")

    result = {
        "model": model,
        "timestamp": datetime.now().isoformat(),
        "info": info,
        "tests": []
    }

    # Test increasing sizes
    test_sizes = [4096, 8192, 16384, 32768, 65536, 131072, 196608]

    prev_tokens = 0
    native_limit = 0
    hardware_limit = 0

    for num_ctx in test_sizes:
        print(f"  Testing {num_ctx//1024}K...", end=" ", flush=True)
        test = test_context(model, num_ctx)
        result["tests"].append({**test, 'num_ctx': num_ctx})

        if not test['success']:
            print(f"FAILED - {test.get('error')}")
            hardware_limit = prev_tokens
            break

        tokens = test['tokens_processed']

        if test.get('truncated'):
            # Found native limit - it's capped at previous token count
            native_limit = tokens
            print(f"NATIVE LIMIT FOUND: {tokens} tokens")
            break

        # Check if we've plateaued (same tokens as last test = native limit)
        if tokens <= prev_tokens * 1.1 and prev_tokens > 0:
            native_limit = tokens
            print(f"PLATEAU at {tokens} tokens (native limit)")
            break

        prev_tokens = tokens
        native_limit = tokens
        print(f"OK ({tokens} tokens, {test['time_s']}s)")

    result["native_context"] = native_limit
    result["hardware_limit"] = hardware_limit if hardware_limit else "not reached"
    result["recommended_input"] = int(native_limit * 0.9)

    print(f"\n  Native context limit: {native_limit} tokens")
    print(f"  Recommended input:    {result['recommended_input']} tokens")

    return result

def test_output_limit(model: str, num_ctx: int = 32768) -> dict:
    """Test maximum output generation."""
    print(f"\n  Testing output limit...")

    prompt = "Write a very long detailed technical document. Include many sections, subsections, and detailed explanations. Continue until you reach your maximum. Section 1:"

    result = {"tests": []}
    max_output = 0

    for num_predict in [1024, 2048, 4096, 8192, 16384]:
        print(f"    num_predict={num_predict}...", end=" ", flush=True)

        try:
            start = time.time()
            resp = requests.post(f'{OLLAMA_URL}/api/generate', json={
                'model': model,
                'prompt': prompt,
                'stream': False,
                'options': {
                    'num_ctx': num_ctx,
                    'num_predict': num_predict
                }
            }, timeout=600)
            elapsed = time.time() - start

            if resp.status_code == 200:
                data = resp.json()
                output_tokens = data.get('eval_count', 0)

                test_record = {
                    'num_predict': num_predict,
                    'actual_output': output_tokens,
                    'time_s': round(elapsed, 2),
                    'tok_per_sec': round(output_tokens / elapsed, 1) if elapsed > 0 else 0
                }
                result["tests"].append(test_record)

                print(f"{output_tokens} tokens ({test_record['tok_per_sec']} tok/s)")

                # If output < requested and not growing, we've hit the limit
                if output_tokens < num_predict and output_tokens <= max_output * 1.1:
                    print(f"    Output limit found: {output_tokens}")
                    break

                max_output = max(max_output, output_tokens)
            else:
                print(f"FAILED")
                break
        except Exception as e:
            print(f"ERROR: {e}")
            break

    result["max_output"] = max_output
    return result

def test_model_complete(model: str) -> dict:
    """Complete test: native context + output limit."""
    result = find_native_limit(model)

    # Use found native limit for output testing
    ctx = min(result.get('native_context', 8192), 131072)
    output_result = test_output_limit(model, ctx)
    result["output"] = output_result

    return result

def main():
    models = sys.argv[1:] if len(sys.argv) > 1 else ["gpt-oss:20b"]

    print("="*60)
    print("SMART CONTEXT & OUTPUT TESTING")
    print(f"Started: {datetime.now().isoformat()}")
    print("="*60)

    all_results = []

    for model in models:
        try:
            result = test_model_complete(model)
            all_results.append(result)
        except Exception as e:
            print(f"Error testing {model}: {e}")
            all_results.append({"model": model, "error": str(e)})

    # Summary
    print("\n" + "="*60)
    print("SUMMARY")
    print("="*60)
    print(f"\n{'Model':<25} {'Native CTX':>12} {'Max Output':>12} {'Output tok/s':>12}")
    print("-"*65)

    for r in all_results:
        if r.get('error'):
            print(f"{r['model']:<25} ERROR")
        else:
            native = r.get('native_context', 0)
            output = r.get('output', {}).get('max_output', 0)
            tok_s = r.get('output', {}).get('tests', [{}])[-1].get('tok_per_sec', 0) if r.get('output', {}).get('tests') else 0
            print(f"{r['model']:<25} {native:>12} {output:>12} {tok_s:>12}")

    # Save
    output_file = f"/home/roctinam/dev/fortemi/docs/research/smart_context_results_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
    with open(output_file, 'w') as f:
        json.dump(all_results, f, indent=2)
    print(f"\nResults saved to: {output_file}")

    return all_results

if __name__ == "__main__":
    main()
