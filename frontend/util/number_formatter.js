export default function commaFormat(amount) {
  let delimiter = ","; // replace comma if desired
  let a = amount.split('.', 2)
  let d = a[1];
  let i = parseInt(a[0]);
  if (isNaN(i)) {
    return '';
  }
  let minus = '';
  if (i < 0) {
    minus = '-';
  }
  i = Math.abs(i);
  let n = new String(i);
  a = [];
  while (n.length > 3) {
    let nn = n.substr(n.length - 3);
    a.unshift(nn);
    n = n.substr(0, n.length - 3);
  }
  if (n.length > 0) {
    a.unshift(n);
  }
  n = a.join(delimiter);
  if (d.length < 1) {
    amount = n;
  } else {
    amount = n + '.' + d;
  }
  amount = minus + amount;
  return amount;
}