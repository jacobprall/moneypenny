import React from 'react'
import { useDispatch, shallowEqual, useSelector } from 'react-redux'
import BillsForm from './bills_form'
import { closeModal } from '../../actions/modal_actions'
import { clearBillErrors, createBill, updateBill, deleteBill } from '../../actions/bill_actions'

export default function bills_form_container() {
  const selectedData = useSelector((state) => ({
    errors: Object.values(state.errors.bill),
    formType: state.ui.modal.formType[0],
    passedBill: state.ui.modal.bill[0],
    user_id: state.session.id
  }), shallowEqual);

  const dispatch = useDispatch();

  let processForm;
  if (selectedData.formType === "new") {
    processForm = (bill) => dispatch(createBill(bill));
  } else {
    processForm = (bill) => dispatch(updateBill(bill));
  }

  const modalCloser = () => dispatch(closeModal());
  const billDeleter = (bill) => {
    bill.recurring = false;
    dispatch(updateBill(bill)).then(() =>
    dispatch(deleteBill(bill.id))).then(() => modalCloser())
  };

  const billErrorsClearer = () => dispatch(clearBillErrors());

  const props = {
    selectedData,
    processForm,
    modalCloser,
    billErrorsClearer,
    billDeleter,
  };

  return (
    <div className="modal-form-container">
      <BillsForm props={props} />
    </div>
  )
}
