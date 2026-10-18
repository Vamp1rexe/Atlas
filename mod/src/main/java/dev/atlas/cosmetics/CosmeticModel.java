package dev.atlas.cosmetics;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.awt.image.BufferedImage;
import java.io.IOException;
import java.io.Reader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import javax.imageio.ImageIO;

/**
 * A cosmetic as a tree of bones. Every bone owns pre-built, greedily merged colored quads
 * (the texture is sampled on the CPU at load time), so a frame only costs a few matrix
 * operations and one geometry submit per bone.
 */
record CosmeticModel(String id, String attach, float ox, float oy, float oz, float scale,
		String animation, float speed, float amount, List<Bone> roots) {

	record Quad(float[] points, int color) { }

	/** Pivot offsets are relative to the parent bone (absolute for roots); angles are degrees. */
	static final class Bone {
		String name;
		float dx, dy, dz;
		float[] base = new float[3], amp = new float[3], move = new float[3];
		float speed = 1f, phase = 0f;
		final List<Quad> quads = new ArrayList<>();
		final List<Bone> children = new ArrayList<>();
		boolean animated;
	}

	static CosmeticModel load(String id, Path modelFile, Path textureFile, float userScale, int tintRgb) throws IOException {
		JsonObject root;
		try (Reader r = Files.newBufferedReader(modelFile)) {
			root = JsonParser.parseReader(r).getAsJsonObject();
		}
		BufferedImage tex = ImageIO.read(textureFile.toFile());
		if (tex == null) throw new IOException("Texture not readable: " + textureFile);

		float[] offset = floats(root.getAsJsonArray("offset"), 3, 0f);
		JsonObject anim = root.has("animation") ? root.getAsJsonObject("animation") : new JsonObject();

		List<Bone> roots = new ArrayList<>();
		if (root.has("bones")) {
			Map<String, Bone> byName = new HashMap<>();
			Map<String, float[]> pivots = new HashMap<>();
			for (JsonElement el : root.getAsJsonArray("bones")) {
				JsonObject jb = el.getAsJsonObject();
				Bone b = new Bone();
				b.name = jb.get("name").getAsString();
				float[] pivot = floats(jb.getAsJsonArray("pivot"), 3, 0f);
				String parent = jb.has("parent") && !jb.get("parent").isJsonNull() ? jb.get("parent").getAsString() : null;
				float[] pp = parent != null ? pivots.getOrDefault(parent, new float[3]) : new float[3];
				b.dx = pivot[0] - pp[0];
				b.dy = pivot[1] - pp[1];
				b.dz = pivot[2] - pp[2];
				if (jb.has("rotation")) {
					JsonObject rot = jb.getAsJsonObject("rotation");
					b.base = floats(rot.getAsJsonArray("base"), 3, 0f);
					b.amp = floats(rot.getAsJsonArray("amp"), 3, 0f);
					b.move = floats(rot.getAsJsonArray("move"), 3, 0f);
					b.speed = rot.has("speed") ? rot.get("speed").getAsFloat() : 1f;
					b.phase = rot.has("phase") ? rot.get("phase").getAsFloat() : 0f;
				}
				if (jb.has("cubes")) {
					for (JsonElement ce : jb.getAsJsonArray("cubes")) {
						addCube(b.quads, tex, tintRgb, ce.getAsJsonObject(), pivot);
					}
				}
				pivots.put(b.name, pivot);
				byName.put(b.name, b);
				Bone parentBone = parent != null ? byName.get(parent) : null;
				(parentBone != null ? parentBone.children : roots).add(b);
			}
		} else {
			// format 1: one anonymous bone, cubes in attach space
			Bone b = new Bone();
			b.name = "root";
			for (JsonElement ce : root.getAsJsonArray("cubes")) {
				addCube(b.quads, tex, tintRgb, ce.getAsJsonObject(), new float[3]);
			}
			roots.add(b);
		}

		return new CosmeticModel(
				id,
				root.has("attach") ? root.get("attach").getAsString() : "head",
				offset[0], offset[1], offset[2],
				(root.has("scale") ? root.get("scale").getAsFloat() : 1f) * userScale,
				anim.has("type") ? anim.get("type").getAsString() : "none",
				anim.has("speed") ? anim.get("speed").getAsFloat() : 1f,
				anim.has("amount") ? anim.get("amount").getAsFloat() : 1f,
				List.copyOf(roots));
	}

	private static float[] floats(JsonArray a, int n, float fallback) {
		float[] out = new float[n];
		for (int i = 0; i < n; i++) out[i] = a != null && i < a.size() ? a.get(i).getAsFloat() : fallback;
		return out;
	}

	private static void addCube(List<Quad> out, BufferedImage tex, int tint, JsonObject cube, float[] pivot) {
		float[] f = floats(cube.getAsJsonArray("from"), 3, 0f);
		float[] t = floats(cube.getAsJsonArray("to"), 3, 0f);
		float x0 = Math.min(f[0], t[0]) - pivot[0], x1 = Math.max(f[0], t[0]) - pivot[0];
		float y0 = Math.min(f[1], t[1]) - pivot[1], y1 = Math.max(f[1], t[1]) - pivot[1];
		float z0 = Math.min(f[2], t[2]) - pivot[2], z1 = Math.max(f[2], t[2]) - pivot[2];
		float w = x1 - x0, h = y1 - y0, d = z1 - z0;

		int[][] uv = new int[6][]; // west north east south up down
		if (cube.has("faces")) {
			JsonObject faces = cube.getAsJsonObject("faces");
			String[] names = {"west", "north", "east", "south", "up", "down"};
			for (int i = 0; i < 6; i++) {
				if (faces.has(names[i])) {
					JsonArray a = faces.getAsJsonArray(names[i]);
					uv[i] = new int[]{a.get(0).getAsInt(), a.get(1).getAsInt(), a.get(2).getAsInt(), a.get(3).getAsInt()};
				}
			}
		} else {
			float[] o = floats(cube.getAsJsonArray("uv"), 2, 0f);
			int u = Math.round(o[0]), v = Math.round(o[1]);
			int iw = Math.round(w), ih = Math.round(h), id = Math.round(d);
			uv[0] = new int[]{u, v + id, u + id, v + id + ih};
			uv[1] = new int[]{u + id, v + id, u + id + iw, v + id + ih};
			uv[2] = new int[]{u + id + iw, v + id, u + 2 * id + iw, v + id + ih};
			uv[3] = new int[]{u + 2 * id + iw, v + id, u + 2 * id + 2 * iw, v + id + ih};
			uv[4] = new int[]{u + id, v, u + id + iw, v + id};
			uv[5] = new int[]{u + id + iw, v, u + id + 2 * iw, v + id};
		}

		// face: origin, texel axis S, texel axis T, extent along S and T, shade
		if (uv[0] != null) face(out, tex, tint, x0, y0, z0, 0, 0, 1, 0, 1, 0, d, h, uv[0], 0.6f);
		if (uv[1] != null) face(out, tex, tint, x1, y0, z0, -1, 0, 0, 0, 1, 0, w, h, uv[1], 0.8f);
		if (uv[2] != null) face(out, tex, tint, x1, y0, z1, 0, 0, -1, 0, 1, 0, d, h, uv[2], 0.6f);
		if (uv[3] != null) face(out, tex, tint, x0, y0, z1, 1, 0, 0, 0, 1, 0, w, h, uv[3], 0.8f);
		if (uv[4] != null) face(out, tex, tint, x0, y0, z0, 1, 0, 0, 0, 0, 1, w, d, uv[4], 1.0f);
		if (uv[5] != null) face(out, tex, tint, x0, y1, z1, 1, 0, 0, 0, 0, -1, w, d, uv[5], 0.5f);
	}

	/** Samples one face into a color grid and merges equal-colored texels into rectangles. */
	private static void face(List<Quad> out, BufferedImage tex, int tint,
			float ox, float oy, float oz, float sx, float sy, float sz, float tx, float ty, float tz,
			float extS, float extT, int[] uv, float shade) {
		int umin = Math.min(uv[0], uv[2]), umax = Math.max(uv[0], uv[2]);
		int vmin = Math.min(uv[1], uv[3]), vmax = Math.max(uv[1], uv[3]);
		boolean flipU = uv[2] < uv[0], flipV = uv[3] < uv[1];
		int nS = umax - umin, nT = vmax - vmin;
		if (nS <= 0 || nT <= 0 || extS <= 0 || extT <= 0) return;

		int tr = (tint >> 16) & 0xFF, tg = (tint >> 8) & 0xFF, tb = tint & 0xFF;
		int[][] grid = new int[nT][nS];
		for (int j = 0; j < nT; j++) {
			for (int i = 0; i < nS; i++) {
				int px = flipU ? umax - 1 - i : umin + i;
				int py = flipV ? vmax - 1 - j : vmin + j;
				if (px < 0 || py < 0 || px >= tex.getWidth() || py >= tex.getHeight()) continue;
				int argb = tex.getRGB(px, py);
				int a = argb >>> 24;
				if (a == 0) continue;
				int r = Math.min(255, Math.round(((argb >> 16) & 0xFF) * shade * tr / 255f));
				int g = Math.min(255, Math.round(((argb >> 8) & 0xFF) * shade * tg / 255f));
				int b = Math.min(255, Math.round((argb & 0xFF) * shade * tb / 255f));
				grid[j][i] = (a << 24) | (r << 16) | (g << 8) | b;
			}
		}

		float stepS = extS / nS, stepT = extT / nT;
		boolean[][] done = new boolean[nT][nS];
		for (int j = 0; j < nT; j++) {
			for (int i = 0; i < nS; i++) {
				int c = grid[j][i];
				if (done[j][i] || (c >>> 24) == 0) continue;
				int w = 1;
				while (i + w < nS && !done[j][i + w] && grid[j][i + w] == c) w++;
				int h = 1;
				grow:
				while (j + h < nT) {
					for (int k = i; k < i + w; k++) {
						if (done[j + h][k] || grid[j + h][k] != c) break grow;
					}
					h++;
				}
				for (int y = j; y < j + h; y++) for (int x = i; x < i + w; x++) done[y][x] = true;

				float[] p = new float[12];
				corner(p, 0, ox, oy, oz, sx, sy, sz, tx, ty, tz, i * stepS, j * stepT);
				corner(p, 1, ox, oy, oz, sx, sy, sz, tx, ty, tz, (i + w) * stepS, j * stepT);
				corner(p, 2, ox, oy, oz, sx, sy, sz, tx, ty, tz, (i + w) * stepS, (j + h) * stepT);
				corner(p, 3, ox, oy, oz, sx, sy, sz, tx, ty, tz, i * stepS, (j + h) * stepT);
				out.add(new Quad(p, c));
			}
		}
	}

	private static void corner(float[] p, int n, float ox, float oy, float oz,
			float sx, float sy, float sz, float tx, float ty, float tz, float s, float t) {
		p[n * 3] = ox + sx * s + tx * t;
		p[n * 3 + 1] = oy + sy * s + ty * t;
		p[n * 3 + 2] = oz + sz * s + tz * t;
	}
}
