using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace SecondScreenDimmer
{
    internal sealed class ModernSlider : Control
    {
        private int minimum;
        private int maximum = 100;
        private int currentValue;
        private bool dragging;
        private bool hovering;
        private bool darkMode;

        internal event EventHandler ValueChanged;

        internal int Minimum
        {
            get { return minimum; }
            set
            {
                minimum = value;
                if (maximum < minimum)
                {
                    maximum = minimum;
                }

                Value = currentValue;
                Invalidate();
            }
        }

        internal int Maximum
        {
            get { return maximum; }
            set
            {
                maximum = value < minimum ? minimum : value;
                Value = currentValue;
                Invalidate();
            }
        }

        internal int Value
        {
            get { return currentValue; }
            set
            {
                int normalized = Math.Max(minimum, Math.Min(maximum, value));
                if (currentValue == normalized)
                {
                    return;
                }

                currentValue = normalized;
                Invalidate();

                EventHandler handler = ValueChanged;
                if (handler != null)
                {
                    handler(this, EventArgs.Empty);
                }
            }
        }

        internal int SmallChange { get; set; }
        internal int LargeChange { get; set; }
        internal int TickFrequency { get; set; }

        internal bool DarkMode
        {
            get { return darkMode; }
            set
            {
                darkMode = value;
                Invalidate();
            }
        }

        internal ModernSlider()
        {
            SmallChange = 1;
            LargeChange = 10;
            Size = new Size(220, 34);
            TabStop = true;
            AccessibleRole = AccessibleRole.Slider;
            Cursor = Cursors.Hand;
            SetStyle(
                ControlStyles.AllPaintingInWmPaint |
                ControlStyles.OptimizedDoubleBuffer |
                ControlStyles.ResizeRedraw |
                ControlStyles.SupportsTransparentBackColor |
                ControlStyles.UserPaint |
                ControlStyles.Selectable,
                true);
            BackColor = Color.Transparent;
        }

        protected override void OnPaint(PaintEventArgs eventArgs)
        {
            base.OnPaint(eventArgs);
            eventArgs.Graphics.SmoothingMode = SmoothingMode.AntiAlias;

            int left = 11;
            int right = Math.Max(left + 1, Width - 11);
            int centerY = Height / 2;
            int trackHeight = 4;
            int trackTop = centerY - (trackHeight / 2);
            int thumbX = ValueToPosition(left, right);

            Color emptyColor = Enabled
                ? (darkMode
                    ? Color.FromArgb(58, 58, 58)
                    : Color.FromArgb(211, 215, 222))
                : (darkMode
                    ? Color.FromArgb(43, 43, 43)
                    : Color.FromArgb(225, 227, 231));
            Color fillColor = Enabled
                ? Color.FromArgb(35, 122, 230)
                : Color.FromArgb(110, 137, 171);

            using (GraphicsPath emptyTrack = CreateRoundedRectangle(
                new Rectangle(left, trackTop, right - left, trackHeight),
                trackHeight / 2))
            using (SolidBrush emptyBrush = new SolidBrush(emptyColor))
            {
                eventArgs.Graphics.FillPath(emptyBrush, emptyTrack);
            }

            if (thumbX > left)
            {
                using (GraphicsPath filledTrack = CreateRoundedRectangle(
                    new Rectangle(left, trackTop, thumbX - left, trackHeight),
                    trackHeight / 2))
                using (SolidBrush fillBrush = new SolidBrush(fillColor))
                {
                    eventArgs.Graphics.FillPath(fillBrush, filledTrack);
                }
            }

            int thumbRadius = dragging ? 9 : (hovering || Focused ? 8 : 7);
            Rectangle thumbBounds = new Rectangle(
                thumbX - thumbRadius,
                centerY - thumbRadius,
                thumbRadius * 2,
                thumbRadius * 2);

            Color thumbColor = Enabled
                ? (darkMode ? Color.FromArgb(247, 247, 247) : Color.White)
                : Color.FromArgb(176, 180, 187);

            using (SolidBrush thumbBrush = new SolidBrush(thumbColor))
            using (Pen thumbBorder = new Pen(
                Focused && Enabled
                    ? Color.FromArgb(35, 122, 230)
                    : (darkMode
                        ? Color.FromArgb(88, 88, 88)
                        : Color.FromArgb(181, 186, 194)),
                Focused ? 2F : 1F))
            {
                eventArgs.Graphics.FillEllipse(thumbBrush, thumbBounds);
                eventArgs.Graphics.DrawEllipse(thumbBorder, thumbBounds);
            }
        }

        protected override void OnMouseDown(MouseEventArgs eventArgs)
        {
            base.OnMouseDown(eventArgs);
            if (!Enabled || eventArgs.Button != MouseButtons.Left)
            {
                return;
            }

            Focus();
            dragging = true;
            Capture = true;
            SetValueFromPosition(eventArgs.X);
            Invalidate();
        }

        protected override void OnMouseMove(MouseEventArgs eventArgs)
        {
            base.OnMouseMove(eventArgs);
            if (dragging)
            {
                SetValueFromPosition(eventArgs.X);
            }
        }

        protected override void OnMouseUp(MouseEventArgs eventArgs)
        {
            base.OnMouseUp(eventArgs);
            if (eventArgs.Button == MouseButtons.Left)
            {
                dragging = false;
                Capture = false;
                Invalidate();
            }
        }

        protected override void OnMouseEnter(EventArgs eventArgs)
        {
            base.OnMouseEnter(eventArgs);
            hovering = true;
            Invalidate();
        }

        protected override void OnMouseLeave(EventArgs eventArgs)
        {
            base.OnMouseLeave(eventArgs);
            hovering = false;
            Invalidate();
        }

        protected override void OnKeyDown(KeyEventArgs eventArgs)
        {
            int nextValue = currentValue;
            switch (eventArgs.KeyCode)
            {
                case Keys.Left:
                case Keys.Down:
                    nextValue -= Math.Max(1, SmallChange);
                    break;
                case Keys.Right:
                case Keys.Up:
                    nextValue += Math.Max(1, SmallChange);
                    break;
                case Keys.PageDown:
                    nextValue -= Math.Max(1, LargeChange);
                    break;
                case Keys.PageUp:
                    nextValue += Math.Max(1, LargeChange);
                    break;
                case Keys.Home:
                    nextValue = minimum;
                    break;
                case Keys.End:
                    nextValue = maximum;
                    break;
                default:
                    base.OnKeyDown(eventArgs);
                    return;
            }

            Value = nextValue;
            eventArgs.Handled = true;
            eventArgs.SuppressKeyPress = true;
        }

        protected override void OnEnabledChanged(EventArgs eventArgs)
        {
            base.OnEnabledChanged(eventArgs);
            Cursor = Enabled ? Cursors.Hand : Cursors.Default;
            Invalidate();
        }

        protected override void OnGotFocus(EventArgs eventArgs)
        {
            base.OnGotFocus(eventArgs);
            Invalidate();
        }

        protected override void OnLostFocus(EventArgs eventArgs)
        {
            base.OnLostFocus(eventArgs);
            Invalidate();
        }

        private int ValueToPosition(int left, int right)
        {
            if (maximum <= minimum)
            {
                return left;
            }

            double ratio = (currentValue - minimum) / (double)(maximum - minimum);
            return left + (int)Math.Round((right - left) * ratio);
        }

        private void SetValueFromPosition(int x)
        {
            int left = 11;
            int right = Math.Max(left + 1, Width - 11);
            double ratio = (Math.Max(left, Math.Min(right, x)) - left) /
                (double)(right - left);
            Value = minimum + (int)Math.Round((maximum - minimum) * ratio);
        }

        private static GraphicsPath CreateRoundedRectangle(Rectangle bounds, int radius)
        {
            int diameter = Math.Max(2, radius * 2);
            diameter = Math.Min(diameter, Math.Min(bounds.Width, bounds.Height));
            diameter = Math.Max(1, diameter);
            GraphicsPath path = new GraphicsPath();
            path.AddArc(bounds.Left, bounds.Top, diameter, diameter, 180, 90);
            path.AddArc(bounds.Right - diameter, bounds.Top, diameter, diameter, 270, 90);
            path.AddArc(bounds.Right - diameter, bounds.Bottom - diameter, diameter, diameter, 0, 90);
            path.AddArc(bounds.Left, bounds.Bottom - diameter, diameter, diameter, 90, 90);
            path.CloseFigure();
            return path;
        }
    }
}
