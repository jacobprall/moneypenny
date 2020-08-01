# == Schema Information
#
# Table name: transactions
#
#  id          :bigint           not null, primary key
#  amount      :decimal(8, 2)    not null
#  category    :string
#  date        :datetime         not null
#  description :string
#  notes       :text
#  created_at  :datetime         not null
#  updated_at  :datetime         not null
#  account_id  :integer          not null
#
# Indexes
#
#  index_transactions_on_account_id  (account_id)
#
class Transaction < ApplicationRecord

  validates :amount, :description, :account_id, :category, :date, presence: true
 
  belongs_to :account,
  foreign_key: :account_id,
  class_name: :Account

  has_one :user,
  through: :account,
  source: :user 

  def isCharge?
    @charge = self.amount < 0 ? true : false
  end

end
