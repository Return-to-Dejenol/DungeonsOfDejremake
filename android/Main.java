package io.github.silvariasereneblossom.dejenol;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/** The game's activity. The game calls pick() to choose the player's own copy of the
 *  original; the chosen files are copied to files/picked/, where the game reads them. */
public class Main extends NativeActivity {
    public void pick() {
        runOnUiThread(new Runnable() {
            public void run() {
                Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*");
                startActivityForResult(i.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true), 1);
            }
        });
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (result != RESULT_OK || data == null) return;
        ClipData clip = data.getClipData();
        int n = clip != null ? clip.getItemCount() : 1;
        File part = new File(getFilesDir(), "picked.part");
        part.mkdirs();
        for (int k = 0; k < n; k++) {
            Uri uri = clip != null ? clip.getItemAt(k).getUri() : data.getData();
            String name = "file" + k;
            try (Cursor c = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
                if (c != null && c.moveToFirst()) name = new File(c.getString(0)).getName();
            } catch (Exception e) {
            }
            try (InputStream in = getContentResolver().openInputStream(uri); OutputStream out = new FileOutputStream(new File(part, name))) {
                byte[] b = new byte[65536];
                for (int r; (r = in.read(b)) > 0; ) out.write(b, 0, r);
            } catch (Exception e) {
            }
        }
        part.renameTo(new File(getFilesDir(), "picked"));
    }
}
