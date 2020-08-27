import * as BillAPIUtil from '../util/bill_api_util'

export const RECEIVE_BILLS = 'RECEIVE_BILLS';
export const RECEIVE_BILL = 'RECEIVE_BILL';
export const REMOVE_BILL = 'REMOVE_BILL';
export const RECEIVE_BILL_ERRORS = 'RECEIVE_BILL_ERRORS';
export const CLEAR_BILL_ERRORS = 'CLEAR_BILL_ERRORS';

export const receiveBills = (bills) => ({
  type: RECEIVE_BILLS,
  bills
});

export const postBill = (bill) => ({
  type: RECEIVE_BILL,
  bill
});

export const patchBill = bill => ({
  type: RECEIVE_BILL,
  bill
});

export const removeBill = bill => ({
  type: REMOVE_BILL,
  bill
});

export const receiveBillErrors = errors => ({
  type: RECEIVE_BILL_ERRORS,
  errors
});

export const clearBillErrors = () => ({
  type: CLEAR_BILL_ERRORS
});

export const requestBills = () => dispatch => (
  BillAPIUtil.fetchBills().then((bills) => dispatch(receiveBills(bills)))
);

export const createBill = bill => dispatch => (
  BillAPIUtil.createBill(bill).then(
    bill => dispatch(postBill(bill)),
    errors => dispatch(receiveBillErrors(errors.responseJSON))
  )
);

export const updateBill = bill => dispatch => (
  BillAPIUtil.updateBill(bill).then(
    bill => dispatch(patchBill(bill)),
    errors => dispatch(receiveBillErrors(errors.responseJSON))
  )
);

export const deleteBill = billId => dispatch => (
  BillAPIUtil.deleteBill(billId).then((bill) => dispatch(removeBill(bill)))
);