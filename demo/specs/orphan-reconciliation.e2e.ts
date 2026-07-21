import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { runDemoActions } from "../runner/actionRunner";
import { parseDemoRunnerOptions } from "../runner/options";

declare const browser: {
  executeAsync<T>(
    callback: (manifestPath: string, done: (result?: T) => void) => void,
    manifestPath: string,
  ): Promise<T>;
  refresh(): Promise<void>;
};
declare const describe: (name: string, callback: () => void) => void;
declare const it: (name: string, callback: () => Promise<void>) => void;

describe("OA Curator orphan reconciliation", () => {
  it("captures reconciliation and collision-safe creation in a disposable Collection", async () => {
    const options = parseDemoRunnerOptions();
    const fixture = createCollisionFixture(options.paths.runDir);
    const result = await browser.executeAsync<{ error?: string }>(
      (collectionManifestPath, done) => {
        const tauri = (
          window as unknown as {
            __TAURI__?: {
              core?: {
                invoke(command: string, args: unknown): Promise<unknown>;
              };
            };
          }
        ).__TAURI__;
        if (!tauri?.core) {
          done({ error: "Tauri API unavailable" });
          return;
        }
        void tauri.core
          .invoke("open_collection_command", {
            request: { path: collectionManifestPath },
          })
          .then(() => tauri.core?.invoke("close_collection_command", {}))
          .then(() => done({}))
          .catch((error: unknown) => done({ error: String(error) }));
      },
      fixture.manifestPath,
    );
    if (result.error) throw new Error(result.error);
    await browser.refresh();

    await runDemoActions(
      [
        { action: "waitForRole", role: "dialog", name: "Open OA Curator" },
        { action: "click", role: "button", name: "Open Orphan Safety Demo" },
        { action: "waitForRole", role: "dialog", name: "Unreferenced Artwork Found" },
        { action: "waitForText", text: "OAC-00002 Hidden Orphan" },
        {
          action: "caption",
          text: "After: OA Curator finds the unreferenced Artwork and asks what to do.",
          durationMs: 1800,
        },
        { action: "screenshot", name: "after-orphan-reconciliation-dialog" },
        { action: "click", role: "button", name: "Leave files untouched" },
        { action: "click", role: "button", name: "Expand gallery Main Gallery" },
        { action: "waitForText", text: "Referenced Artwork" },
        { action: "click", role: "button", name: "New Artwork" },
        { action: "waitForText", text: "Artwork created" },
        { action: "waitForText", text: "OAC-00003" },
        {
          action: "caption",
          text: "After: ignored OAC-00002 stays untouched and the new Artwork uses OAC-00003.",
          durationMs: 1800,
        },
        { action: "screenshot", name: "after-collision-skipped" },
      ],
      options,
    );

    if (readFileSync(fixture.orphanManifestPath, "utf8") !== fixture.orphanContents) {
      throw new Error("The ignored orphan manifest changed during the after-state demo");
    }
    const collection = JSON.parse(readFileSync(fixture.manifestPath, "utf8")) as {
      artworks: { id: string }[];
    };
    const ids = collection.artworks.map((artwork) => artwork.id);
    if (ids.join(",") !== "OAC-00001,OAC-00003") {
      throw new Error(
        "Expected Collection Artwork IDs OAC-00001,OAC-00003; received " + ids.join(","),
      );
    }
  });
});

function createCollisionFixture(runDir: string) {
  const root = path.join(runDir, "workspace", "Orphan Safety Demo");
  const galleryFolder = path.join(root, "galleries", "Main Gallery");
  const referencedFolder = path.join(root, "artworks", "OAC-00001");
  const orphanFolder = path.join(root, "artworks", "OAC-00002");
  for (const folder of [galleryFolder, referencedFolder, orphanFolder]) {
    mkdirSync(folder, { recursive: true });
  }
  writeJson(path.join(root, ".oacollection"), {
    schema_version: "0.1",
    id: "collection-orphan-safety-demo",
    name: "Orphan Safety Demo",
    galleries: [
      {
        id: "gallery-orphan-safety-demo",
        name: "Main Gallery",
        path: "galleries/Main Gallery/.oagallery",
      },
    ],
    artworks: [
      {
        id: "OAC-00001",
        path: "artworks/OAC-00001/.oaartwork",
      },
    ],
  });
  writeJson(path.join(galleryFolder, ".oagallery"), {
    schema_version: "0.1",
    id: "gallery-orphan-safety-demo",
    name: "Main Gallery",
    artworks: [{ id: "OAC-00001" }],
  });
  writeJson(path.join(referencedFolder, ".oaartwork"), {
    schema_version: "0.1",
    id: "OAC-00001",
    title: "Referenced Artwork",
    files: [],
  });
  const orphanManifestPath = path.join(orphanFolder, ".oaartwork");
  const orphanManifest = {
    schema_version: "0.1",
    id: "OAC-00002",
    title: "Hidden Orphan",
    files: [],
  };
  writeJson(orphanManifestPath, orphanManifest);
  return {
    manifestPath: path.join(root, ".oacollection"),
    orphanManifestPath,
    orphanContents: JSON.stringify(orphanManifest, null, 2) + "\n",
  };
}

function writeJson(filePath: string, value: unknown) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}
