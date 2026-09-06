import importlib.util
from pathlib import Path
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import threading

spec = importlib.util.spec_from_file_location(
    "cutover", Path(__file__).resolve().parents[1] / "scripts/ci/verify-hotm-cutover.py")
cutover = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cutover)


def container(name, network="shared", network_id="abc", aliases=None):
    return {"Name": name, "State": {"Running": True}, "NetworkSettings": {
        "Networks": {network: {"NetworkID": network_id, "Aliases": aliases}}}}


class TopologyTests(unittest.TestCase):
    def test_shared_network_requires_server_alias(self):
        server = container("server", aliases=["fortemi"])
        ui, proxy = container("ui"), container("proxy")
        self.assertEqual(cutover.check_topology([server, ui, proxy]), [])
        server["NetworkSettings"]["Networks"]["shared"]["Aliases"] = None
        self.assertEqual(len(cutover.check_topology([server, ui, proxy])), 2)

    def test_recreated_server_on_different_project_network_fails(self):
        server = container("server", network="new_default", aliases=["fortemi"])
        self.assertEqual(len(cutover.check_topology(
            [server, container("ui"), container("proxy")])), 2)

    def test_network_name_alone_is_not_continuity(self):
        self.assertTrue(cutover.check_topology([
            container("server", network_id="new", aliases=["fortemi"]), container("ui")]))

    def test_stopped_container_cannot_pass(self):
        server = container("server", aliases=["fortemi"])
        server["State"]["Running"] = False
        self.assertTrue(cutover.check_topology([server, container("ui")]))


class HttpTests(unittest.TestCase):
    def test_real_http_states_and_redirect_rejection(self):
        class Handler(BaseHTTPRequestHandler):
            status = 200
            payload = b'{"status":"healthy"}'

            def do_GET(self):
                self.send_response(self.status)
                if self.status == 302:
                    self.send_header("Location", "/login")
                self.end_headers()
                self.wfile.write(self.payload)

            def log_message(self, *_args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        origin = f"http://127.0.0.1:{server.server_port}"
        try:
            self.assertEqual(cutover.probe(origin, "/health"), ("reachable", 200))
            Handler.payload = b'<html>login</html>'
            self.assertEqual(cutover.probe(origin, "/health"), ("invalid_response", 200))
            for code, expected in [(401, "auth_required"), (403, "auth_required"),
                                   (410, "http_error"), (502, "http_error"),
                                   (302, "http_error")]:
                Handler.status = code
                self.assertEqual(cutover.probe(origin, "/health"), (expected, code))
            Handler.status = 200
            Handler.payload = b'{"schema_version":"1","api":{}}'
            self.assertEqual(cutover.probe(origin, "/api/v1/system/compatibility"),
                             ("reachable", 200))
            Handler.payload = b'{"status":"healthy"}'
            self.assertEqual(cutover.probe(origin, "/api/v1/system/compatibility"),
                             ("invalid_response", 200))
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
        self.assertEqual(cutover.probe(origin, "/health"), ("unreachable", None))


if __name__ == "__main__":
    unittest.main()
