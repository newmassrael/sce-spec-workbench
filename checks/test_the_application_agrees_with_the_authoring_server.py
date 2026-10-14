# SPDX-License-Identifier: LicenseRef-SCE-Spec-Workbench-NonCommercial-1.0 OR LicenseRef-SCE-Commercial
# SPDX-FileCopyrightText: Copyright (c) 2026 newmassrael
"""The application and the authoring server it runs agree on what each names by hand.

The authoring server is SCE's (`sce/tools/authoring`, in the submodule) and the application is
this repository's. Four things are written on both sides, in two languages, and are the same only
for as long as something holds them to each other. This does, against the one revision of SCE the
application is built with:

- the tools a generation may use: the application approves them unattended and enables them in the
  client (`AUTHOR_TOOLS`), and the server advertises them and refuses the rest;
- the variable that tells the server which work a run is for (`WORK_SCOPE`): a name that drifted
  on one side leaves a server that answers for every work of the folder and says nothing;
- the Python packages the `.deb` declares, because it carries the server and not the Python it
  runs in: the server says which it needs (`sce_author.needs`);
- the packages the CI machine that tries the installer is given, since a machine that already has
  a module would pass an installer that does not ask for it.

`scripts/verify_installed_app.sh` holds the `.deb` that was BUILT to the same list. Run with
`scripts/check_installer.sh`, or `PYTHONPATH=sce/tools/authoring python3 -m unittest discover -s
checks`.
"""

from __future__ import annotations

import json
import os
import pathlib
import re
import unittest
from unittest.mock import patch

from sce_author import mcp, needs

REPO = pathlib.Path(__file__).resolve().parents[1]
CLIENT_RUN = REPO / "core" / "src" / "client_run.rs"
TAURI_CONFIG = REPO / "src-tauri" / "tauri.conf.json"
INSTALLER_WORKFLOW = REPO / ".github" / "workflows" / "installer.yml"


class TheToolsAndTheScope(unittest.TestCase):
    def test_the_tools_a_generation_may_use_are_the_ones_the_application_approves(self):
        source = CLIENT_RUN.read_text(encoding="utf-8")
        declared = re.search(r"pub const AUTHOR_TOOLS: \[&str; \d+\] = \[(.*?)\];", source, re.S)
        self.assertIsNotNone(declared, "AUTHOR_TOOLS is not where this test looks for it")
        approved = set(re.findall(r'"([a-z_]+)"', declared.group(1)))
        self.assertEqual(approved, set(mcp.WORKBENCH_TOOLS))

    def test_the_variable_every_client_sets_is_the_one_that_scopes_the_server(self):
        source = CLIENT_RUN.read_text(encoding="utf-8")
        declared = re.search(r'const WORK_SCOPE: &str = "([A-Z_]+)";', source)
        self.assertIsNotNone(declared, "WORK_SCOPE is not where this test looks for it")
        with patch.dict(os.environ):
            # Unscoped, the server advertises more than a generation may use: the control that
            # shows the answer below comes from the variable and not from the server's default.
            os.environ.pop(declared.group(1), None)
            unscoped = mcp.handle({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})
            os.environ[declared.group(1)] = "assigned-work"
            scoped = mcp.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        self.assertNotEqual(mcp.WORKBENCH_TOOLS,
                            {t["name"] for t in unscoped["result"]["tools"]})
        self.assertEqual(mcp.WORKBENCH_TOOLS,
                         {t["name"] for t in scoped["result"]["tools"]})


class TheInstallerDeclaresWhatTheServerNeeds(unittest.TestCase):
    def test_the_deb_depends_on_python_and_on_a_package_for_every_module_the_server_needs(self):
        config = json.loads(TAURI_CONFIG.read_text(encoding="utf-8"))
        depends = config["bundle"]["linux"]["deb"]["depends"]

        self.assertIn("python3", depends)
        for need in needs.NEEDS:
            self.assertIn(need.debian, depends,
                          f"the server needs {need.name} ({need.module}), and the .deb does not "
                          f"declare {need.debian}")

    def test_the_machine_that_tries_the_installer_is_given_the_same_packages(self):
        workflow = INSTALLER_WORKFLOW.read_text(encoding="utf-8")

        for need in needs.NEEDS:
            self.assertTrue(need.debian in workflow,
                            f"{INSTALLER_WORKFLOW.name} does not install {need.debian}, so the "
                            f"application it starts could not find {need.name}")


if __name__ == "__main__":
    unittest.main()
