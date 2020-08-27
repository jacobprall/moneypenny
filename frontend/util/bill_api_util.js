
export const fetchBills = () => (
  $.ajax({
    url: '/api/bills'
  })
);

export const createBill = bill => (
  $.ajax({
    url: 'api/bills',
    method: 'POST',
    data: {
      bill
    }
  })
);

export const updateBill = bill => (
  $.ajax({
    url: `api/bills/${bill.id}`,
    method: 'PATCH',
    data: {
      bill
    }
  })
);

export const deleteBill = billId => (
  $.ajax({
    url: `api/bills/${billId}`,
    method: 'DELETE'
  })
);
