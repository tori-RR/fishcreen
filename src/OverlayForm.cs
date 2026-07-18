using System.Drawing;
using System.Windows.Forms;

namespace SecondScreenDimmer
{
    internal sealed class OverlayForm : Form
    {
        internal OverlayForm()
        {
            AutoScaleMode = AutoScaleMode.None;
            BackColor = Color.Black;
            FormBorderStyle = FormBorderStyle.None;
            MaximizeBox = false;
            MinimizeBox = false;
            ShowIcon = false;
            ShowInTaskbar = false;
            StartPosition = FormStartPosition.Manual;
            TopMost = true;
        }

        protected override bool ShowWithoutActivation
        {
            get { return true; }
        }

        protected override CreateParams CreateParams
        {
            get
            {
                CreateParams parameters = base.CreateParams;
                parameters.ExStyle |= NativeMethods.WsExToolWindow;
                parameters.ExStyle |= NativeMethods.WsExNoActivate;
                return parameters;
            }
        }

        internal void Cover(Rectangle bounds, double opacity)
        {
            SetOverlayOpacity(opacity);
            Bounds = bounds;

            if (!Visible)
            {
                Show();
            }

            NativeMethods.SetWindowPos(
                Handle,
                NativeMethods.HwndTopmost,
                bounds.X,
                bounds.Y,
                bounds.Width,
                bounds.Height,
                NativeMethods.SwpNoActivate | NativeMethods.SwpShowWindow);
        }

        internal void SetOverlayOpacity(double opacity)
        {
            double clamped = System.Math.Max(0.01, System.Math.Min(1.0, opacity));
            if (System.Math.Abs(Opacity - clamped) > 0.001)
            {
                Opacity = clamped;
            }
        }

        internal void Uncover()
        {
            if (Visible)
            {
                Hide();
            }
        }

        protected override void WndProc(ref Message message)
        {
            if (message.Msg == NativeMethods.WmMouseActivate)
            {
                message.Result = new System.IntPtr(NativeMethods.MaNoActivate);
                return;
            }

            base.WndProc(ref message);
        }
    }
}
