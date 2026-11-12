package io.github.silvariasereneblossom.dejenol;

import android.app.NativeActivity;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.ClipData;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageInstaller;
import android.database.Cursor;
import android.graphics.Rect;
import android.net.Uri;
import android.os.Build;
import android.provider.OpenableColumns;
import android.view.View;
import android.view.WindowInsets;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/** The game's activity. The game calls pick() to choose the player's own copy of the
 *  original; the chosen files are copied to files/picked/, where the game reads them. */
public class Main extends NativeActivity {
    /** The on-screen keyboard's height in pixels; 0 while it's hidden. */
    public int keyboard() {
        View root = getWindow().getDecorView();
        if (Build.VERSION.SDK_INT >= 30) {
            WindowInsets w = root.getRootWindowInsets();
            return w == null ? 0 : w.getInsets(WindowInsets.Type.ime()).bottom;
        }
        Rect r = new Rect();
        root.getWindowVisibleDisplayFrame(r);
        return Math.max(0, root.getHeight() - r.bottom);
    }

    /** Installs a downloaded update: a PackageInstaller session the system confirms with the
     *  player, then replaces the app (same signing key), keeping its files. */
    public void install(final String apk) {
        final String done = getPackageName() + ".INSTALLED";
        BroadcastReceiver r = new BroadcastReceiver() {
            public void onReceive(Context c, Intent i) {
                Intent ask = i.getParcelableExtra(Intent.EXTRA_INTENT);
                if (i.getIntExtra(PackageInstaller.EXTRA_STATUS, 1) == PackageInstaller.STATUS_PENDING_USER_ACTION && ask != null)
                    startActivity(ask.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            }
        };
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(r, new IntentFilter(done), Context.RECEIVER_NOT_EXPORTED);
        else registerReceiver(r, new IntentFilter(done));
        try {
            PackageInstaller pi = getPackageManager().getPackageInstaller();
            int id = pi.createSession(new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL));
            try (PackageInstaller.Session s = pi.openSession(id)) {
                try (InputStream in = new FileInputStream(apk); OutputStream out = s.openWrite("update.apk", 0, new File(apk).length())) {
                    byte[] b = new byte[65536];
                    for (int n; (n = in.read(b)) > 0; ) out.write(b, 0, n);
                    s.fsync(out);
                }
                int flags = PendingIntent.FLAG_UPDATE_CURRENT | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
                Intent back = new Intent(done).setPackage(getPackageName());
                s.commit(PendingIntent.getBroadcast(this, id, back, flags).getIntentSender());
            }
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }

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
