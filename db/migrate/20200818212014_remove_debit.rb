class RemoveDebit < ActiveRecord::Migration[5.2]
  def change
    remove_column :accounts, :debit
  end
end
