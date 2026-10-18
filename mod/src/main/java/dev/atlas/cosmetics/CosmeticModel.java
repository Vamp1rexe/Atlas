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
import java.util.List;
import javax.imageio.ImageIO;

/**
 * A cosmetic turned into colored quads. The texture is sampled on the CPU, one quad per
 * opaque texel, so no GPU texture has to be registered.
 */
record CosmeticModel(
		String id, String attach, float ox, float oy, float oz, float scale,
		String animation, float speed, float amount, List<Quad> quads) {

	record Quad(float[] points, int color) { }

	static CosmeticModel load(String id, Path modelFile, Path textureFile) throws IOException {
		JsonObject root;
		try (Reader r = Files.newBufferedReader(modelFile)) {
			root = JsonParser.parseReader(r).getAsJsonObject();
		}
		BufferedImage tex = ImageIO.read(textureFile.toFile());
		if (tex == null) throw new IOException("Texture not readable: " + textureFile);

		float[] offset = floats(root.getAsJsonArray("offset"), 3, 0f);
		JsonObject anim = root.has("animation") ? root.getAsJsonObject("animation") : new JsonObject();

		List<Quad> quads = new ArrayList<>();
		for (JsonElement el : root.getAsJsonArray("cubes")) {
			JsonObject cube = el.getAsJsonObject();
			addCube(quads, tex, floats(cube.getAsJsonArray("from"), 3, 0f), floats(cube.getAsJsonArray("to"), 3, 0f),
					floats(cube.getAsJsonArray("uv"), 2, 0f));
		}

		return new CosmeticModel(
				id,
				root.has("attach") ? root.get("attach").getAsString() : "head",
				offset[0], offset[1], offset[2],
				root.has("scale") ? root.get("scale").getAsFloat() : 1f,
				anim.has("type") ? anim.get("type").getAsString() : "none",
				anim.has("speed") ? anim.get("speed").getAsFloat() : 1f,
				anim.has("amount") ? anim.get("amount").getAsFloat() : 1f,
				List.copyOf(quads));
	}

	private static float[] floats(JsonArray a, int n, float fallback) {
		float[] out = new float[n];
		for (int i = 0; i < n; i++) out[i] = a != null && i < a.size() ? a.get(i).getAsFloat() : fallback;
		return out;
	}

	private static void addCube(List<Quad> out, BufferedImage tex, float[] from, float[] to, float[] uv) {
		float x0 = from[0], y0 = from[1], z0 = from[2], x1 = to[0], y1 = to[1], z1 = to[2];
		int w = Math.round(x1 - x0), h = Math.round(y1 - y0), d = Math.round(z1 - z0);
		int u = Math.round(uv[0]), v = Math.round(uv[1]);

		// west, north, east, south, up, down: origin, texel axes, texel counts, texture origin, shade
		face(out, tex, x0, y0, z0, 0, 0, 1, 0, 1, 0, d, h, u, v + d, 0.6f);
		face(out, tex, x1, y0, z0, -1, 0, 0, 0, 1, 0, w, h, u + d, v + d, 0.8f);
		face(out, tex, x1, y0, z1, 0, 0, -1, 0, 1, 0, d, h, u + d + w, v + d, 0.6f);
		face(out, tex, x0, y0, z1, 1, 0, 0, 0, 1, 0, w, h, u + 2 * d + w, v + d, 0.8f);
		face(out, tex, x0, y0, z0, 1, 0, 0, 0, 0, 1, w, d, u + d, v, 1.0f);
		face(out, tex, x0, y1, z1, 1, 0, 0, 0, 0, -1, w, d, u + d + w, v, 0.5f);
	}

	private static void face(List<Quad> out, BufferedImage tex,
			float ox, float oy, float oz, float sx, float sy, float sz, float tx, float ty, float tz,
			int nS, int nT, int texU, int texV, float shade) {
		for (int i = 0; i < nS; i++) {
			for (int j = 0; j < nT; j++) {
				int px = texU + i, py = texV + j;
				if (px < 0 || py < 0 || px >= tex.getWidth() || py >= tex.getHeight()) continue;
				int argb = tex.getRGB(px, py);
				int a = argb >>> 24;
				if (a == 0) continue;
				int r = Math.min(255, Math.round(((argb >> 16) & 0xFF) * shade));
				int g = Math.min(255, Math.round(((argb >> 8) & 0xFF) * shade));
				int b = Math.min(255, Math.round((argb & 0xFF) * shade));
				float[] p = new float[12];
				corner(p, 0, ox, oy, oz, sx, sy, sz, tx, ty, tz, i, j);
				corner(p, 1, ox, oy, oz, sx, sy, sz, tx, ty, tz, i + 1, j);
				corner(p, 2, ox, oy, oz, sx, sy, sz, tx, ty, tz, i + 1, j + 1);
				corner(p, 3, ox, oy, oz, sx, sy, sz, tx, ty, tz, i, j + 1);
				out.add(new Quad(p, (a << 24) | (r << 16) | (g << 8) | b));
			}
		}
	}

	private static void corner(float[] p, int n,
			float ox, float oy, float oz, float sx, float sy, float sz, float tx, float ty, float tz, int i, int j) {
		p[n * 3] = ox + sx * i + tx * j;
		p[n * 3 + 1] = oy + sy * i + ty * j;
		p[n * 3 + 2] = oz + sz * i + tz * j;
	}
}
