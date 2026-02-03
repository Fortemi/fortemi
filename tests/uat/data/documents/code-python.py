"""Sample Python module for testing code chunking."""

from typing import List, Optional
import json


class DataProcessor:
    """Processes data with various transformations."""

    def __init__(self, config: dict):
        self.config = config
        self.cache = {}

    def process(self, data: List[dict]) -> List[dict]:
        """Process a list of data items."""
        return [self._transform(item) for item in data]

    def _transform(self, item: dict) -> dict:
        """Transform a single item."""
        if not isinstance(item, dict):
            raise ValueError("Item must be a dictionary")

        # Apply transformations
        transformed = {
            "id": item.get("id"),
            "processed": True,
            "original": item
        }

        # Cache result
        if "id" in item:
            self.cache[item["id"]] = transformed

        return transformed


def main():
    """Main entry point."""
    config = {"mode": "strict", "validate": True}
    processor = DataProcessor(config)

    test_data = [
        {"id": 1, "value": "test1"},
        {"id": 2, "value": "test2"}
    ]

    result = processor.process(test_data)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
